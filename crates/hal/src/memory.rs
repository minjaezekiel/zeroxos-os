//! Memory interface — page mapping, DMA, cache maintenance.

use core::ptr::NonNull;

/// Page size on the current architecture (4 KB on both ARM64 and x86_64).
pub const PAGE_SIZE: usize = 4096;

/// Page-shift bits (log2 of PAGE_SIZE).
pub const PAGE_SHIFT: u32 = 12;

/// Huge page sizes supported by the kernel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HugePageSize {
    /// 2 MB huge page
    Mb2,
    /// 1 GB huge page (x86_64 only; ARM64 with optional block-mapping)
    Gb1,
}

impl HugePageSize {
    pub fn bytes(&self) -> usize {
        match self {
            HugePageSize::Mb2 => 2 * 1024 * 1024,
            HugePageSize::Gb1 => 1024 * 1024 * 1024,
        }
    }
}

/// Memory protection flags applied to a page mapping.
#[derive(Debug, Clone, Copy, Default)]
pub struct PageFlags(pub u32);

impl PageFlags {
    pub const READ: Self = Self(1 << 0);
    pub const WRITE: Self = Self(1 << 1);
    pub const EXEC: Self = Self(1 << 2);
    pub const USER: Self = Self(1 << 3);
    pub const WRITE_THROUGH: Self = Self(1 << 4);
    pub const CACHE_DISABLE: Self = Self(1 << 5);
    pub const GLOBAL: Self = Self(1 << 6);

    pub fn or(self, other: Self) -> Self { Self(self.0 | other.0) }
    pub fn contains(self, other: Self) -> bool { (self.0 & other.0) == other.0 }
}

/// Physical memory address.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PhysAddr(pub u64);

/// Virtual memory address.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct VirtAddr(pub u64);

/// A region of physical memory suitable for DMA.
pub struct DmaRegion {
    pub phys: PhysAddr,
    pub virt: NonNull<u8>,
    pub size: usize,
    pub is_cache_coherent: bool,
}

/// Map a virtual page to a physical page with the given protection flags.
///
/// # Safety
/// Caller must ensure `phys` is a valid physical address and `virt` is not
/// already mapped.
pub unsafe fn map_page(virt: VirtAddr, phys: PhysAddr, flags: PageFlags) {
    crate::arch::map_page(virt, phys, flags);
}

/// Unmap a virtual page.
///
/// # Safety
/// Caller must ensure `virt` is currently mapped.
pub unsafe fn unmap_page(virt: VirtAddr) {
    crate::arch::unmap_page(virt);
}

/// Allocate a contiguous region of physical memory suitable for DMA.
pub fn allocate_dma(size: usize, align: usize) -> Option<DmaRegion> {
    crate::arch::allocate_dma(size, align)
}

/// Flush the data cache for the given virtual address range.
///
/// # Safety
/// Required on non-cache-coherent platforms (some ARM systems) before
/// handing memory to a DMA-capable device.
pub unsafe fn flush_cache(addr: VirtAddr, size: usize) {
    crate::arch::flush_cache(addr, size);
}

/// Invalidate the data cache for the given virtual address range.
///
/// # Safety
/// Required on non-cache-coherent platforms after a device writes to memory.
pub unsafe fn invalidate_cache(addr: VirtAddr, size: usize) {
    crate::arch::invalidate_cache(addr, size);
}
