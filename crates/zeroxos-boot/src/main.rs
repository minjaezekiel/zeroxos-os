//! zeroxos bare-metal entry point (x86_64), booted by the **Limine** bootloader.
//!
//! Limine loads this ELF, sets up a 64-bit long-mode environment with a stack
//! and a higher-half direct map, and jumps to [`kmain`]. We declare a set of
//! Limine *requests* (below); the bootloader fills in their responses before
//! handing control over. From those we learn the physical memory map, the
//! Higher-Half Direct Map (HHDM) offset, and the framebuffer.
//!
//! The same ELF, wrapped in a Limine ISO, boots in QEMU and on real UEFI/BIOS
//! hardware — see `docs/MANUAL.md` and `scripts/mk-iso.sh`.

#![no_std]
#![no_main]

use limine::request::{
    BootloaderInfoRequest, FramebufferRequest, HhdmRequest, MemmapRequest, StackSizeRequest,
};
use limine::{BaseRevision, RequestsEndMarker, RequestsStartMarker};

// --- Limine requests --------------------------------------------------------
//
// All requests live in the `.limine_requests` section (see linker/x86_64.ld),
// bounded by start/end markers so the bootloader can scan them, and marked
// `#[used]` + KEEP so neither the compiler nor the linker drops them.

/// Ask Limine for a comfortable 64 KiB stack.
const STACK_SIZE: u64 = 64 * 1024;

#[used]
#[link_section = ".limine_requests_start"]
static REQUESTS_START: RequestsStartMarker = RequestsStartMarker::new();

// Request Limine base revision 3 (the well-supported modern protocol with
// virtual/HHDM pointers). The crate's default (`new()`) asks for revision 6,
// which Limine 9.6.7 does not support.
#[used]
#[link_section = ".limine_requests"]
static BASE_REVISION: BaseRevision = BaseRevision::with_revision(3);

#[used]
#[link_section = ".limine_requests"]
static BOOTLOADER_INFO: BootloaderInfoRequest = BootloaderInfoRequest::new();

#[used]
#[link_section = ".limine_requests"]
static STACK_SIZE_REQUEST: StackSizeRequest = StackSizeRequest::new(STACK_SIZE);

#[used]
#[link_section = ".limine_requests"]
static HHDM_REQUEST: HhdmRequest = HhdmRequest::new();

#[used]
#[link_section = ".limine_requests"]
static MEMMAP_REQUEST: MemmapRequest = MemmapRequest::new();

#[used]
#[link_section = ".limine_requests"]
static FRAMEBUFFER_REQUEST: FramebufferRequest = FramebufferRequest::new();

#[used]
#[link_section = ".limine_requests_end"]
static REQUESTS_END: RequestsEndMarker = RequestsEndMarker::new();

// --- Frame allocator hook for the HAL page-table code -----------------------

/// Allocate one physical frame from the kernel buddy allocator; returns its
/// physical byte address (0 on failure). Registered with the HAL so
/// `map_page` can allocate intermediate page tables.
fn alloc_frame() -> u64 {
    let mut kernel = zerox_kernel::KERNEL.lock();
    match kernel.memory.alloc_page() {
        Some(frame) => frame.0 * 4096, // PageFrame holds a PFN
        None => 0,
    }
}

/// Return a physical frame to the buddy allocator.
fn free_frame(phys: u64) {
    let mut kernel = zerox_kernel::KERNEL.lock();
    kernel.memory.free_page(zerox_kernel::memory::PageFrame(phys / 4096));
}

/// Kernel entry point. Limine jumps here with interrupts disabled, in long mode,
/// on the stack it set up for us.
#[no_mangle]
pub extern "C" fn kmain() -> ! {
    // 1. Serial console + logger first, so everything below is visible and any
    //    panic during bring-up is reported.
    zerox_kernel::serial::init_logger(log::LevelFilter::Info);
    log::info!("[boot] zeroxos bootstrapping via Limine...");

    // 2. Confirm the bootloader speaks a base revision we support.
    if !BASE_REVISION.is_supported() {
        log::error!("[boot] unsupported Limine base revision — halting");
        halt();
    }
    if let Some(info) = BOOTLOADER_INFO.response() {
        log::info!("[boot] bootloader: {} {}", info.name(), info.version());
    }

    // 3. Higher-Half Direct Map: tells us where physical memory is visible in
    //    our address space. The M4 page-table walker needs this base.
    let hhdm_offset = HHDM_REQUEST.response().map(|r| r.offset).unwrap_or(0);
    log::info!("[boot] HHDM offset = {:#x}", hhdm_offset);
    hal::arch::set_direct_map_base(hhdm_offset);
    hal::arch::set_frame_allocator(alloc_frame, free_frame);

    // 4. Early kernel heap (bootstrap; grown from real frames once the buddy
    //    allocator has the memory map).
    unsafe {
        let base = core::ptr::addr_of_mut!(EARLY_HEAP) as usize;
        zerox_kernel::heap::init_kernel_heap(base, EARLY_HEAP_SIZE);
    }

    // 5. Register the bootloader's usable RAM with the kernel buddy allocator.
    let mut usable_bytes: u64 = 0;
    let mut regions: u64 = 0;
    if let Some(memmap) = MEMMAP_REQUEST.response() {
        let mut kernel = zerox_kernel::KERNEL.lock();
        for entry in memmap.entries() {
            if entry.type_ == limine::memmap::MEMMAP_USABLE {
                kernel.memory.register_region(entry.base, entry.length);
                usable_bytes += entry.length;
                regions += 1;
            }
        }
    }
    log::info!(
        "[boot] usable RAM: {} MiB across {} regions",
        usable_bytes / 1024 / 1024,
        regions
    );

    // 6. CPU tables (GDT/IDT/TSS + syscalls) and HAL init.
    unsafe {
        zerox_kernel::arch::x86_64::init();
        hal::init();
    }
    log::info!("[boot] arch + HAL initialized");

    // 7. Boot the kernel proper (logs each subsystem over serial).
    match zerox_kernel::KERNEL.lock().boot() {
        Ok(()) => log::info!("[boot] kernel subsystems online"),
        Err(e) => {
            log::error!("[boot] kernel boot failed: {}", e);
            halt();
        }
    }

    // 8. Prove the framebuffer handoff: report its mode and paint the screen so
    //    there is a visible sign of life on real hardware / the QEMU window.
    if let Some(fb_resp) = FRAMEBUFFER_REQUEST.response() {
        if let Some(fb) = fb_resp.framebuffers().first() {
            log::info!(
                "[boot] framebuffer: {}x{} {}bpp",
                fb.width,
                fb.height,
                fb.bpp
            );
            paint_banner(fb);
        }
    }

    log::info!("[boot] zeroxos v0.1 booted. Welcome.");
    halt();
}

/// Fill the framebuffer with zeroxos's deep-blue so a successful boot is
/// visible even without a text console.
fn paint_banner(fb: &limine::framebuffer::Framebuffer) {
    // Assume the common 32-bpp little-endian BGRX layout Limine provides.
    let pixel: u32 = 0x0011_2244; // deep blue
    let addr = fb.address() as *mut u8;
    let pitch = fb.pitch as usize;
    let width = fb.width as usize;
    let height = fb.height as usize;
    if fb.bpp != 32 {
        return;
    }
    for y in 0..height {
        let row = unsafe { addr.add(y * pitch) } as *mut u32;
        for x in 0..width {
            unsafe { row.add(x).write_volatile(pixel) };
        }
    }
}

/// Halt the CPU forever (interrupts disabled).
fn halt() -> ! {
    loop {
        unsafe { core::arch::asm!("cli; hlt", options(nomem, nostack)) };
    }
}

/// Size of the early boot heap carved out of `.bss` (1 MiB), used until the
/// buddy allocator has the real memory map.
const EARLY_HEAP_SIZE: usize = 1024 * 1024;

/// The early boot heap arena. Lives in `.bss`; Limine zeroes it.
static mut EARLY_HEAP: [u8; EARLY_HEAP_SIZE] = [0; EARLY_HEAP_SIZE];
