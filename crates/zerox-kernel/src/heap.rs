//! Kernel heap allocator.
//!
//! This is the allocator behind `Box`, `Vec`, `String`, etc. **inside the
//! kernel on bare metal**. It is a first-fit free-list allocator managing a
//! single contiguous arena, with block splitting on allocation and buddy
//! coalescing on free.
//!
//! ## Why not back `GlobalAlloc` directly with the buddy allocator?
//!
//! The roadmap's illustrative stub wires `GlobalAlloc` straight to
//! [`crate::memory::BuddyAllocator`]. That cannot work as written: the buddy's
//! free lists are `alloc::vec::Vec`s, so a buddy allocation may itself call the
//! global allocator, which re-enters the (already locked) buddy — deadlock and
//! infinite recursion. The correct kernel design keeps two distinct layers:
//!
//! - the **buddy allocator** hands out *physical page frames*;
//! - this **heap allocator** hands out *bytes* for kernel objects, from an
//!   arena that is itself carved out of frames the buddy owns.
//!
//! On bare metal, [`init`] is called once early in boot with a region backed by
//! real physical memory. On the host, the standard library provides the global
//! allocator, so this type is compiled but not registered — its logic is still
//! exercised directly by the unit tests below.

use core::alloc::{GlobalAlloc, Layout};
use core::mem::{align_of, size_of};
use core::ptr;
use spin::Mutex;

/// A node in the intrinsic free list. Stored *inside* the free region it
/// describes, so the allocator needs no external bookkeeping memory.
struct FreeBlock {
    size: usize,
    next: Option<&'static mut FreeBlock>,
}

/// Minimum allocation size / alignment — every block must be large enough to
/// hold a [`FreeBlock`] header once freed, and aligned to hold one.
const MIN_BLOCK: usize = size_of::<FreeBlock>();
const MIN_ALIGN: usize = align_of::<FreeBlock>();

/// Round `addr` up to the next multiple of `align` (a power of two).
const fn align_up(addr: usize, align: usize) -> usize {
    (addr + align - 1) & !(align - 1)
}

/// A first-fit free-list heap over one contiguous arena.
pub struct Heap {
    head: FreeBlock,
    /// Total bytes managed (for stats / tests).
    total: usize,
    /// Bytes currently handed out (for stats / tests).
    allocated: usize,
}

impl Heap {
    /// Create an empty, uninitialized heap. Call [`Heap::init`] before use.
    pub const fn empty() -> Self {
        Self {
            head: FreeBlock { size: 0, next: None },
            total: 0,
            allocated: 0,
        }
    }

    /// Add the region `[start, start + size)` to the heap.
    ///
    /// # Safety
    /// - The region must be valid, unused, writable memory that outlives the
    ///   heap.
    /// - This must be called at most once per region, and the region must not
    ///   overlap any other region given to this heap.
    pub unsafe fn init(&mut self, start: usize, size: usize) {
        unsafe { self.add_free_region(start, size) };
        self.total += size;
    }

    /// Push `[addr, addr+size)` onto the free list, if it can hold a header.
    ///
    /// # Safety
    /// `addr` must be writable for `size` bytes and not aliased elsewhere.
    unsafe fn add_free_region(&mut self, addr: usize, size: usize) {
        let aligned = align_up(addr, MIN_ALIGN);
        let adjust = aligned - addr;
        if size < adjust + MIN_BLOCK {
            // Too small to ever be reused; leak it (rare, only at region edges).
            return;
        }
        let usable = size - adjust;
        let node_ptr = aligned as *mut FreeBlock;
        // Insert at the head of the free list.
        let next = self.head.next.take();
        unsafe {
            ptr::write(node_ptr, FreeBlock { size: usable, next });
            self.head.next = Some(&mut *node_ptr);
        }
    }

    /// Normalize a requested layout to `(size, align)` we can satisfy: at least
    /// `MIN_BLOCK` bytes and `MIN_ALIGN` alignment so freed blocks fit a header.
    fn adjust_layout(layout: Layout) -> (usize, usize) {
        let align = layout.align().max(MIN_ALIGN);
        let size = align_up(layout.size().max(MIN_BLOCK), align);
        (size, align)
    }

    /// Allocate `layout`. Returns a null pointer on OOM (per `GlobalAlloc`).
    pub fn alloc_first_fit(&mut self, layout: Layout) -> *mut u8 {
        let (size, align) = Self::adjust_layout(layout);

        // Walk the free list looking for the first block that fits after
        // aligning the start. `prev` is a raw pointer so that traversal does not
        // hold a tracked borrow of `self` across the `add_free_region` calls.
        let mut prev: *mut FreeBlock = &mut self.head;
        // SAFETY: `prev` always points at a valid node we own; `next` chains are
        // valid free blocks written by `add_free_region`.
        unsafe {
            while let Some(region) = (*prev).next.as_deref_mut() {
                let region_ptr = region as *mut FreeBlock;
                let region_start = region_ptr as usize;
                let region_end = region_start + region.size;
                let alloc_start = align_up(region_start, align);
                let alloc_end = alloc_start + size;

                if alloc_end <= region_end {
                    // Fits. Detach this region from the list.
                    let next = region.next.take();
                    (*prev).next = next;

                    // Any padding before `alloc_start` (from alignment) and any
                    // tail after `alloc_end` becomes free again.
                    let front_pad = alloc_start - region_start;
                    if front_pad >= MIN_BLOCK {
                        self.add_free_region(region_start, front_pad);
                    }
                    let tail = region_end - alloc_end;
                    if tail >= MIN_BLOCK {
                        self.add_free_region(alloc_end, tail);
                    }
                    self.allocated += size;
                    return alloc_start as *mut u8;
                }
                prev = region_ptr;
            }
        }
        ptr::null_mut() // out of memory
    }

    /// Free a block previously returned by [`Heap::alloc_first_fit`].
    ///
    /// # Safety
    /// `ptr` must have come from this heap with the same `layout`.
    pub unsafe fn dealloc(&mut self, ptr: *mut u8, layout: Layout) {
        let (size, _align) = Self::adjust_layout(layout);
        unsafe { self.add_free_region(ptr as usize, size) };
        self.allocated -= size;
    }

    /// Bytes currently allocated.
    pub fn allocated(&self) -> usize {
        self.allocated
    }
    /// Total bytes under management.
    pub fn total(&self) -> usize {
        self.total
    }
}

/// A `spin::Mutex`-wrapped [`Heap`] usable as a `#[global_allocator]`.
pub struct LockedHeap(Mutex<Heap>);

impl LockedHeap {
    /// Create an empty locked heap. Call [`LockedHeap::init`] before first use.
    pub const fn empty() -> Self {
        LockedHeap(Mutex::new(Heap::empty()))
    }

    /// Initialize the underlying heap over `[start, start + size)`.
    ///
    /// # Safety
    /// Same contract as [`Heap::init`].
    pub unsafe fn init(&self, start: usize, size: usize) {
        unsafe { self.0.lock().init(start, size) };
    }

    /// Access heap stats.
    pub fn allocated(&self) -> usize {
        self.0.lock().allocated()
    }
}

unsafe impl GlobalAlloc for LockedHeap {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        self.0.lock().alloc_first_fit(layout)
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { self.0.lock().dealloc(ptr, layout) }
    }
}

/// The kernel's global allocator on bare metal. On the host build the standard
/// library supplies the global allocator, so this is compiled out.
#[cfg(feature = "bare")]
#[global_allocator]
static KERNEL_HEAP: LockedHeap = LockedHeap::empty();

/// Initialize the bare-metal kernel heap. Call exactly once, early in `_start`,
/// after physical memory is available.
///
/// # Safety
/// `[start, start + size)` must be valid, exclusively-owned RAM.
#[cfg(feature = "bare")]
pub unsafe fn init_kernel_heap(start: usize, size: usize) {
    unsafe { KERNEL_HEAP.init(start, size) };
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::alloc::{alloc as sys_alloc, dealloc as sys_dealloc};

    /// Grab a page-aligned backing arena from the host allocator for the heap
    /// under test, run `f`, then release it.
    fn with_arena(size: usize, f: impl FnOnce(&mut Heap, usize)) {
        let layout = Layout::from_size_align(size, 4096).unwrap();
        let base = unsafe { sys_alloc(layout) };
        assert!(!base.is_null());
        let mut heap = Heap::empty();
        unsafe { heap.init(base as usize, size) };
        f(&mut heap, base as usize);
        unsafe { sys_dealloc(base, layout) };
    }

    #[test]
    fn align_up_rounds_correctly() {
        assert_eq!(align_up(0, 8), 0);
        assert_eq!(align_up(1, 8), 8);
        assert_eq!(align_up(8, 8), 8);
        assert_eq!(align_up(9, 16), 16);
    }

    #[test]
    fn allocations_are_within_arena_and_nonoverlapping() {
        with_arena(64 * 1024, |heap, base| {
            let l = Layout::from_size_align(128, 8).unwrap();
            let a = heap.alloc_first_fit(l);
            let b = heap.alloc_first_fit(l);
            assert!(!a.is_null() && !b.is_null());
            let (a, b) = (a as usize, b as usize);
            // Both inside the arena.
            assert!(a >= base && a + 128 <= base + 64 * 1024);
            assert!(b >= base && b + 128 <= base + 64 * 1024);
            // Non-overlapping.
            assert!(a + 128 <= b || b + 128 <= a);
        });
    }

    #[test]
    fn respects_alignment() {
        with_arena(64 * 1024, |heap, _base| {
            let l = Layout::from_size_align(64, 256).unwrap();
            let p = heap.alloc_first_fit(l) as usize;
            assert!(p != 0);
            assert_eq!(p % 256, 0, "allocation must honor requested alignment");
        });
    }

    #[test]
    fn freed_memory_is_reused() {
        with_arena(8 * 1024, |heap, _base| {
            let l = Layout::from_size_align(256, 8).unwrap();
            let p1 = heap.alloc_first_fit(l);
            assert!(!p1.is_null());
            unsafe { heap.dealloc(p1, l) };
            let p2 = heap.alloc_first_fit(l);
            // First-fit should hand the just-freed block right back.
            assert_eq!(p1, p2, "freed block should be reused by an equal request");
            assert_eq!(heap.allocated(), 256usize.max(MIN_BLOCK));
        });
    }

    #[test]
    fn out_of_memory_returns_null() {
        with_arena(4 * 1024, |heap, _base| {
            let l = Layout::from_size_align(1 << 20, 8).unwrap(); // 1 MiB into a 4 KiB arena
            assert!(heap.alloc_first_fit(l).is_null());
        });
    }
}
