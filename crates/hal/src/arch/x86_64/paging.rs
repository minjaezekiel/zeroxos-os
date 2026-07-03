//! x86_64 4-level page tables (PML4 → PDPT → PD → PT).
//!
//! A 48-bit canonical virtual address is split into four 9-bit table indices
//! and a 12-bit page offset:
//!
//! ```text
//!  47        39 38        30 29        21 20        12 11         0
//! +------------+------------+------------+------------+------------+
//! |  PML4 idx  |  PDPT idx  |   PD idx   |   PT idx   |   offset   |
//! +------------+------------+------------+------------+------------+
//! ```
//!
//! ## Testability
//!
//! Walking the tables requires two capabilities the *hardware* provides but a
//! host test cannot: turning a physical frame address into something
//! dereferenceable, and allocating fresh zeroed frames for intermediate tables.
//! Both are abstracted as traits ([`PhysMapper`], [`FrameAllocator`]), so the
//! entire map / unmap / translate algorithm is exercised on the host against an
//! in-memory arena of tables. On bare metal the same algorithm runs against
//! `CR3` with a physical direct-map.

use crate::memory::{PageFlags, PhysAddr, VirtAddr};

/// Entries per table.
pub const ENTRY_COUNT: usize = 512;
/// 4 KiB page size.
pub const PAGE_SIZE: u64 = 4096;

// Page-table entry flag bits (Intel SDM Vol 3 §4.5).
pub const PTE_PRESENT: u64 = 1 << 0;
pub const PTE_WRITABLE: u64 = 1 << 1;
pub const PTE_USER: u64 = 1 << 2;
pub const PTE_WRITE_THROUGH: u64 = 1 << 3;
pub const PTE_CACHE_DISABLE: u64 = 1 << 4;
pub const PTE_HUGE: u64 = 1 << 7; // PS bit: maps a 2 MiB / 1 GiB page
pub const PTE_GLOBAL: u64 = 1 << 8;
pub const PTE_NO_EXECUTE: u64 = 1 << 63;

/// Mask selecting the physical frame address out of a PTE (bits 12..=51).
pub const PTE_ADDR_MASK: u64 = 0x000F_FFFF_FFFF_F000;

/// Translate HAL-neutral [`PageFlags`] into x86 PTE flag bits. `PRESENT` is
/// always set; a missing `WRITE`/`EXEC` maps to read-only / no-execute.
pub fn pte_flags_from(flags: PageFlags) -> u64 {
    let mut bits = PTE_PRESENT;
    if flags.contains(PageFlags::WRITE) {
        bits |= PTE_WRITABLE;
    }
    if flags.contains(PageFlags::USER) {
        bits |= PTE_USER;
    }
    if flags.contains(PageFlags::WRITE_THROUGH) {
        bits |= PTE_WRITE_THROUGH;
    }
    if flags.contains(PageFlags::CACHE_DISABLE) {
        bits |= PTE_CACHE_DISABLE;
    }
    if flags.contains(PageFlags::GLOBAL) {
        bits |= PTE_GLOBAL;
    }
    // NX unless explicitly executable. (Requires EFER.NXE, enabled at boot.)
    if !flags.contains(PageFlags::EXEC) {
        bits |= PTE_NO_EXECUTE;
    }
    bits
}

/// Split a virtual address into `[PML4, PDPT, PD, PT]` indices.
pub const fn virt_to_indices(virt: u64) -> [usize; 4] {
    [
        ((virt >> 39) & 0x1FF) as usize,
        ((virt >> 30) & 0x1FF) as usize,
        ((virt >> 21) & 0x1FF) as usize,
        ((virt >> 12) & 0x1FF) as usize,
    ]
}

/// The 12-bit offset within a 4 KiB page.
pub const fn page_offset(virt: u64) -> u64 {
    virt & 0xFFF
}

/// One 512-entry page table, page-aligned as the hardware requires.
#[derive(Clone)]
#[repr(C, align(4096))]
pub struct PageTable {
    pub entries: [u64; ENTRY_COUNT],
}

impl PageTable {
    pub const fn empty() -> Self {
        PageTable { entries: [0; ENTRY_COUNT] }
    }
}

/// Turns a physical frame address into a pointer to the [`PageTable`] living
/// there. On bare metal this is `phys + direct_map_base`; in tests it looks the
/// frame up in an arena.
///
/// # Safety
/// The returned pointer must be valid and uniquely owned for the duration of a
/// table walk.
pub unsafe trait PhysMapper {
    unsafe fn table_at(&self, phys: u64) -> *mut PageTable;
}

/// Supplies fresh, zeroed physical frames for intermediate tables.
pub trait FrameAllocator {
    /// Allocate a zeroed 4 KiB frame, returning its physical address.
    fn alloc_zeroed(&mut self) -> Option<u64>;
    /// Return a frame to the allocator.
    fn free(&mut self, phys: u64);
}

/// Errors from a mapping operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MapError {
    /// Out of frames while allocating an intermediate table.
    OutOfFrames,
    /// The target virtual address is already mapped.
    AlreadyMapped,
    /// A huge-page (PS) mapping blocks the requested 4 KiB walk.
    HugePagePresent,
}

/// Map `virt` → `phys` (a single 4 KiB page) in the tree rooted at physical
/// address `root`, allocating intermediate tables as needed.
///
/// # Safety
/// `root` must be a valid PML4 frame reachable through `mapper`, and the caller
/// must ensure exclusive access to the tables (no concurrent walkers).
pub unsafe fn map_page_in(
    root: u64,
    virt: VirtAddr,
    phys: PhysAddr,
    flags: PageFlags,
    mapper: &impl PhysMapper,
    alloc: &mut impl FrameAllocator,
) -> Result<(), MapError> {
    let idx = virt_to_indices(virt.0);
    let pte_flags = pte_flags_from(flags);

    // Descend PML4 → PDPT → PD, creating tables as required.
    let mut table_phys = root;
    for level in 0..3 {
        let table = unsafe { &mut *mapper.table_at(table_phys) };
        let entry = table.entries[idx[level]];
        if entry & PTE_PRESENT == 0 {
            let new = alloc.alloc_zeroed().ok_or(MapError::OutOfFrames)?;
            // Intermediate entries are permissive; the leaf PTE decides final
            // permissions. Include USER so user leaves remain reachable.
            table.entries[idx[level]] = new | PTE_PRESENT | PTE_WRITABLE | PTE_USER;
            table_phys = new;
        } else if entry & PTE_HUGE != 0 {
            return Err(MapError::HugePagePresent);
        } else {
            table_phys = entry & PTE_ADDR_MASK;
        }
    }

    // Leaf PT: install the mapping.
    let pt = unsafe { &mut *mapper.table_at(table_phys) };
    let leaf = &mut pt.entries[idx[3]];
    if *leaf & PTE_PRESENT != 0 {
        return Err(MapError::AlreadyMapped);
    }
    *leaf = (phys.0 & PTE_ADDR_MASK) | pte_flags;
    Ok(())
}

/// Translate `virt` to its physical address in the tree rooted at `root`, or
/// `None` if unmapped. Handles 4 KiB, 2 MiB (PD huge), and 1 GiB (PDPT huge).
///
/// # Safety
/// As [`map_page_in`].
pub unsafe fn translate_in(
    root: u64,
    virt: VirtAddr,
    mapper: &impl PhysMapper,
) -> Option<PhysAddr> {
    let idx = virt_to_indices(virt.0);
    let mut table_phys = root;
    // level 0 = PML4, 1 = PDPT (1 GiB huge), 2 = PD (2 MiB huge), 3 = PT
    for level in 0..3 {
        let table = unsafe { &*mapper.table_at(table_phys) };
        let entry = table.entries[idx[level]];
        if entry & PTE_PRESENT == 0 {
            return None;
        }
        if entry & PTE_HUGE != 0 {
            // Huge page at this level: base + offset within the huge page.
            let shift = 39 - 9 * level as u64; // PDPT:30, PD:21
            let size = 1u64 << shift;
            let base = entry & PTE_ADDR_MASK;
            return Some(PhysAddr(base + (virt.0 & (size - 1))));
        }
        table_phys = entry & PTE_ADDR_MASK;
    }
    let pt = unsafe { &*mapper.table_at(table_phys) };
    let leaf = pt.entries[idx[3]];
    if leaf & PTE_PRESENT == 0 {
        return None;
    }
    Some(PhysAddr((leaf & PTE_ADDR_MASK) | page_offset(virt.0)))
}

/// Remove the 4 KiB mapping for `virt`, returning the physical frame it pointed
/// at (if any). Intermediate tables are left in place (freed lazily later).
///
/// # Safety
/// As [`map_page_in`].
pub unsafe fn unmap_page_in(
    root: u64,
    virt: VirtAddr,
    mapper: &impl PhysMapper,
) -> Option<PhysAddr> {
    let idx = virt_to_indices(virt.0);
    let mut table_phys = root;
    for level in 0..3 {
        let table = unsafe { &*mapper.table_at(table_phys) };
        let entry = table.entries[idx[level]];
        if entry & PTE_PRESENT == 0 || entry & PTE_HUGE != 0 {
            return None;
        }
        table_phys = entry & PTE_ADDR_MASK;
    }
    let pt = unsafe { &mut *mapper.table_at(table_phys) };
    let leaf = &mut pt.entries[idx[3]];
    if *leaf & PTE_PRESENT == 0 {
        return None;
    }
    let frame = *leaf & PTE_ADDR_MASK;
    *leaf = 0;
    Some(PhysAddr(frame))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::boxed::Box;
    use std::cell::UnsafeCell;
    use std::vec::Vec;

    /// A fixed pool of page-table frames with **stable** addresses (a boxed
    /// slice, never reallocated). "Physical address" = frame index × PAGE_SIZE.
    /// Frame 0 is the root PML4.
    ///
    /// The mapper borrows the pool (shared); the allocator borrows only a
    /// separate bump counter — disjoint borrows, so there is no aliasing UB.
    struct Pool {
        tables: Box<[UnsafeCell<PageTable>]>,
    }
    impl Pool {
        fn new() -> Self {
            let mut v = Vec::with_capacity(64);
            for _ in 0..64 {
                v.push(UnsafeCell::new(PageTable::empty()));
            }
            Pool { tables: v.into_boxed_slice() }
        }
    }
    unsafe impl PhysMapper for Pool {
        unsafe fn table_at(&self, phys: u64) -> *mut PageTable {
            self.tables[(phys / PAGE_SIZE) as usize].get()
        }
    }

    /// Bump allocator over frame indices. Starts at 1 (frame 0 is the root).
    struct Bump {
        next: u64,
    }
    impl Bump {
        fn new() -> Self {
            Bump { next: 1 }
        }
    }
    impl FrameAllocator for Bump {
        fn alloc_zeroed(&mut self) -> Option<u64> {
            let phys = self.next * PAGE_SIZE;
            self.next += 1;
            Some(phys) // pool frames start zeroed and are never reused in tests
        }
        fn free(&mut self, _phys: u64) {}
    }

    const ROOT: u64 = 0;

    #[test]
    fn indices_decompose_known_address() {
        // Distinct indices 1,2,3,4 and offset 0x123.
        let v = (1u64 << 39) | (2u64 << 30) | (3u64 << 21) | (4u64 << 12) | 0x123;
        assert_eq!(virt_to_indices(v), [1, 2, 3, 4]);
        assert_eq!(page_offset(v), 0x123);
    }

    #[test]
    fn flags_set_present_write_and_nx() {
        let ro = pte_flags_from(PageFlags::READ);
        assert_eq!(ro & PTE_PRESENT, PTE_PRESENT);
        assert_eq!(ro & PTE_WRITABLE, 0);
        assert_eq!(ro & PTE_NO_EXECUTE, PTE_NO_EXECUTE); // not exec → NX
        let rwx = pte_flags_from(PageFlags::WRITE.or(PageFlags::EXEC));
        assert_eq!(rwx & PTE_WRITABLE, PTE_WRITABLE);
        assert_eq!(rwx & PTE_NO_EXECUTE, 0); // exec → not NX
    }

    #[test]
    fn map_then_translate_roundtrips() {
        let pool = Pool::new();
        let mut bump = Bump::new();
        let virt = VirtAddr(0x0000_7F00_1234_5000);
        let phys = PhysAddr(0x1_0000);
        unsafe {
            map_page_in(ROOT, virt, phys, PageFlags::WRITE, &pool, &mut bump).unwrap();
            // Same page, different offset → phys base + offset.
            let translated = translate_in(ROOT, VirtAddr(virt.0 + 0x40), &pool);
            assert_eq!(translated, Some(PhysAddr(phys.0 + 0x40)));
        }
    }

    #[test]
    fn double_map_is_rejected() {
        let pool = Pool::new();
        let mut bump = Bump::new();
        let v = VirtAddr(0x1000);
        unsafe {
            map_page_in(ROOT, v, PhysAddr(0x5000), PageFlags::READ, &pool, &mut bump).unwrap();
            assert_eq!(
                map_page_in(ROOT, v, PhysAddr(0x6000), PageFlags::READ, &pool, &mut bump),
                Err(MapError::AlreadyMapped)
            );
        }
    }

    #[test]
    fn unmap_clears_and_returns_frame() {
        let pool = Pool::new();
        let mut bump = Bump::new();
        let v = VirtAddr(0x2000);
        unsafe {
            map_page_in(ROOT, v, PhysAddr(0xABC000), PageFlags::READ, &pool, &mut bump).unwrap();
            assert_eq!(unmap_page_in(ROOT, v, &pool), Some(PhysAddr(0xABC000)));
            assert_eq!(translate_in(ROOT, v, &pool), None);
            // Second unmap finds nothing.
            assert_eq!(unmap_page_in(ROOT, v, &pool), None);
        }
    }

    #[test]
    fn distinct_pages_map_independently() {
        let pool = Pool::new();
        let mut bump = Bump::new();
        let a = VirtAddr(0x0000_1000_0000_0000);
        let b = VirtAddr(0x0000_2000_0000_0000);
        unsafe {
            map_page_in(ROOT, a, PhysAddr(0xAA000), PageFlags::READ, &pool, &mut bump).unwrap();
            map_page_in(ROOT, b, PhysAddr(0xBB000), PageFlags::READ, &pool, &mut bump).unwrap();
            assert_eq!(translate_in(ROOT, a, &pool), Some(PhysAddr(0xAA000)));
            assert_eq!(translate_in(ROOT, b, &pool), Some(PhysAddr(0xBB000)));
        }
    }

    #[test]
    fn translate_unmapped_is_none() {
        let pool = Pool::new();
        assert_eq!(
            unsafe { translate_in(ROOT, VirtAddr(0xDEAD_0000), &pool) },
            None
        );
    }
}
