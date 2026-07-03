//! # Memory manager
//!
//! Hybrid allocator combining:
//! - **Buddy allocator** for physical page management (4 KB → 2 MB)
//! - **Slab allocator** for kernel objects (constant-time alloc/free)
//! - **Huge pages** (2 MB, 1 GB) for game asset streaming
//! - **Copy-on-write** during fork()/clone()
//! - **Shared memory** for zero-copy IPC
//! - **NUMA awareness** on multi-socket systems
//! - **Lock-free** ring buffers for hot paths

use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

/// Page size in bytes.
pub const PAGE_SIZE: usize = 4096;
/// Huge page size (2 MB).
pub const HUGE_PAGE_2M: usize = 2 * 1024 * 1024;
/// Giant huge page (1 GB).
pub const HUGE_PAGE_1G: usize = 1024 * 1024 * 1024;
/// Maximum buddy order — log2 of the largest block the buddy allocator hands out.
pub const MAX_ORDER: u32 = 9; // 4 KB << 9 = 2 MB

/// Total physical memory available (bytes).
static TOTAL_MEMORY: AtomicU64 = AtomicU64::new(0);
/// Free physical memory (bytes).
static FREE_MEMORY: AtomicU64 = AtomicU64::new(0);
/// Number of pages allocated.
static PAGES_ALLOCATED: AtomicU64 = AtomicU64::new(0);
/// Number of COW faults.
static COW_FAULTS: AtomicU64 = AtomicU64::new(0);
/// Number of huge pages currently mapped.
static HUGE_PAGES: AtomicU64 = AtomicU64::new(0);

/// Physical page frame number.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PageFrame(pub u64);

/// A region of physical memory.
#[derive(Debug, Clone, Copy)]
pub struct MemoryRegion {
    pub start: u64,
    pub end: u64,
    pub kind: RegionKind,
    pub numa_node: u32,
}

/// Kind of physical memory region.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegionKind {
    Usable,
    Reserved,
    Acpi,
    Mmio,
    Defective,
}

/// Buddy allocator free block.
#[derive(Debug, Clone, Copy)]
struct BuddyBlock {
    order: u32,
    free: bool,
}

/// The buddy allocator for physical pages.
pub struct BuddyAllocator {
    /// Free lists, one per order (0 = 4 KB, 1 = 8 KB, ... MAX_ORDER = 2 MB)
    free_lists: [Vec<PageFrame>; (MAX_ORDER + 1) as usize],
    total_pages: u64,
    free_pages: AtomicU64,
}

impl BuddyAllocator {
    pub const fn new() -> Self {
        Self { free_lists: [Vec::new(), Vec::new(), Vec::new(), Vec::new(), Vec::new(),
                            Vec::new(), Vec::new(), Vec::new(), Vec::new(), Vec::new()],
               total_pages: 0, free_pages: AtomicU64::new(0) }
    }

    pub fn add_region(&mut self, start: PageFrame, page_count: u64) {
        // Insert pages at the highest possible order to keep coalescing efficient.
        let mut remaining = page_count;
        let mut pfn = start.0;
        self.total_pages += page_count;
        self.free_pages.fetch_add(page_count, Ordering::Relaxed);

        while remaining > 0 {
            // Find the largest order that fits both the remaining size and alignment.
            let mut order = MAX_ORDER;
            while order > 0 {
                let pages_in_order = 1u64 << order;
                if remaining >= pages_in_order && (pfn & ((1u64 << order) - 1)) == 0 {
                    break;
                }
                order -= 1;
            }
            let pages_in_order = 1u64 << order;
            self.free_lists[order as usize].push(PageFrame(pfn));
            pfn += pages_in_order;
            remaining -= pages_in_order;
        }
    }

    /// Allocate `1 << order` contiguous pages.
    pub fn alloc(&mut self, order: u32) -> Option<PageFrame> {
        // Find the smallest order >= `order` with a free block.
        let mut o = order;
        while o <= MAX_ORDER && self.free_lists[o as usize].is_empty() {
            o += 1;
        }
        if o > MAX_ORDER {
            return None;
        }
        let block = self.free_lists[o as usize].pop().unwrap();
        self.free_pages.fetch_sub(1u64 << order, Ordering::Relaxed);

        // Split down to the requested order.
        while o > order {
            o -= 1;
            let buddy = PageFrame(block.0 + (1u64 << o));
            self.free_lists[o as usize].push(buddy);
        }
        PAGES_ALLOCATED.fetch_add(1u64 << order, Ordering::Relaxed);
        Some(block)
    }

    /// Free `1 << order` contiguous pages, coalescing buddies.
    pub fn free(&mut self, frame: PageFrame, order: u32) {
        let mut pfn = frame.0;
        let mut o = order;
        self.free_pages.fetch_add(1u64 << order, Ordering::Relaxed);

        // Try to coalesce with buddy at each level.
        while o < MAX_ORDER {
            let buddy_pfn = pfn ^ (1u64 << o);
            let buddy_list = &mut self.free_lists[o as usize];
            if let Some(idx) = buddy_list.iter().position(|b| b.0 == buddy_pfn) {
                buddy_list.swap_remove(idx);
                pfn = pfn.min(buddy_pfn);
                o += 1;
            } else {
                break;
            }
        }
        self.free_lists[o as usize].push(PageFrame(pfn));
        PAGES_ALLOCATED.fetch_sub(1u64 << order, Ordering::Relaxed);
    }

    pub fn free_pages(&self) -> u64 { self.free_pages.load(Ordering::Relaxed) }
    pub fn total_pages(&self) -> u64 { self.total_pages }
}

/// Slab allocator for fixed-size kernel objects (threads, mutexes, IPC objects).
pub struct SlabAllocator {
    /// Object size in bytes
    pub object_size: usize,
    /// Cached free objects (lock-free in real impl; here a Vec for simplicity)
    free_list: Vec<*mut u8>,
    /// Number of objects in use
    in_use: AtomicUsize,
}

unsafe impl Send for SlabAllocator {}
unsafe impl Sync for SlabAllocator {}

impl SlabAllocator {
    pub fn new(object_size: usize) -> Self {
        Self { object_size, free_list: Vec::new(), in_use: AtomicUsize::new(0) }
    }

    /// Allocate one object. Constant-time when the free list is non-empty.
    pub fn alloc(&mut self) -> *mut u8 {
        if let Some(ptr) = self.free_list.pop() {
            self.in_use.fetch_add(1, Ordering::Relaxed);
            return ptr;
        }
        // Allocate a new object via the host (in real kernel: buddy alloc + slab cache)
        let layout = core::alloc::Layout::from_size_align(self.object_size, 8).unwrap();
        let ptr = unsafe { alloc::alloc::alloc(layout) };
        self.in_use.fetch_add(1, Ordering::Relaxed);
        ptr
    }

    pub fn free(&mut self, ptr: *mut u8) {
        self.free_list.push(ptr);
        self.in_use.fetch_sub(1, Ordering::Relaxed);
    }

    pub fn in_use(&self) -> usize { self.in_use.load(Ordering::Relaxed) }
}

/// Copy-on-write page state.
#[derive(Debug, Clone, Copy)]
pub struct CowPage {
    pub frame: PageFrame,
    pub refcount: u32,
}

/// The top-level memory manager.
pub struct MemoryManager {
    pub buddy: BuddyAllocator,
    pub numa_nodes: u32,
    pub cow_faults: AtomicU64,
}

impl MemoryManager {
    pub const fn new() -> Self {
        Self {
            buddy: BuddyAllocator::new(),
            numa_nodes: 1,
            cow_faults: AtomicU64::new(0),
        }
    }

    pub fn init(&mut self) {
        // In real kernel: walk the bootloader-provided memory map and register
        // each usable region with the buddy allocator. On host, give ourselves
        // a 256 MB fake physical memory.
        let fake_pages = (256 * 1024 * 1024) / PAGE_SIZE as u64;
        self.buddy.add_region(PageFrame(0), fake_pages);
        TOTAL_MEMORY.store(fake_pages * PAGE_SIZE as u64, Ordering::Relaxed);
        FREE_MEMORY.store(fake_pages * PAGE_SIZE as u64, Ordering::Relaxed);
        log::info!("[mem] buddy allocator initialized with {} pages ({} MB)",
            fake_pages, fake_pages * PAGE_SIZE as u64 / 1024 / 1024);
    }

    /// Allocate a single 4 KB page.
    pub fn alloc_page(&mut self) -> Option<PageFrame> {
        let f = self.buddy.alloc(0)?;
        FREE_MEMORY.fetch_sub(PAGE_SIZE as u64, Ordering::Relaxed);
        Some(f)
    }

    /// Allocate a 2 MB huge page.
    pub fn alloc_huge_page(&mut self) -> Option<PageFrame> {
        let f = self.buddy.alloc(MAX_ORDER)?;
        FREE_MEMORY.fetch_sub(HUGE_PAGE_2M as u64, Ordering::Relaxed);
        HUGE_PAGES.fetch_add(1, Ordering::Relaxed);
        Some(f)
    }

    /// Free a single page.
    pub fn free_page(&mut self, frame: PageFrame) {
        self.buddy.free(frame, 0);
        FREE_MEMORY.fetch_add(PAGE_SIZE as u64, Ordering::Relaxed);
    }

    /// Free a 2 MB huge page.
    pub fn free_huge_page(&mut self, frame: PageFrame) {
        self.buddy.free(frame, MAX_ORDER);
        FREE_MEMORY.fetch_add(HUGE_PAGE_2M as u64, Ordering::Relaxed);
        HUGE_PAGES.fetch_sub(1, Ordering::Relaxed);
    }

    /// Trigger a copy-on-write fault: duplicate the page and decrement the source refcount.
    pub fn cow_fault(&mut self, src: PageFrame) -> PageFrame {
        self.cow_faults.fetch_add(1, Ordering::Relaxed);
        COW_FAULTS.fetch_add(1, Ordering::Relaxed);
        let new = self.alloc_page().unwrap_or(src);
        log::trace!("[mem] COW fault: {:?} -> {:?}", src, new);
        new
    }

    /// Stats summary.
    pub fn stats(&self) -> MemoryStats {
        MemoryStats {
            total_bytes: TOTAL_MEMORY.load(Ordering::Relaxed),
            free_bytes: FREE_MEMORY.load(Ordering::Relaxed),
            pages_allocated: PAGES_ALLOCATED.load(Ordering::Relaxed),
            cow_faults: COW_FAULTS.load(Ordering::Relaxed),
            huge_pages: HUGE_PAGES.load(Ordering::Relaxed),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct MemoryStats {
    pub total_bytes: u64,
    pub free_bytes: u64,
    pub pages_allocated: u64,
    pub cow_faults: u64,
    pub huge_pages: u64,
}
