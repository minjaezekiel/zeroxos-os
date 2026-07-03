/* zeroxos x86_64 kernel linker script (Limine boot protocol).
 *
 * The kernel is a higher-half kernel, linked into the top 2 GiB of the address
 * space (matching `code-model: kernel`). Limine loads it, maps it at this
 * virtual address, sets up a stack, and jumps to `kmain`.
 */

OUTPUT_FORMAT(elf64-x86-64)
OUTPUT_ARCH(i386:x86-64)
ENTRY(kmain)

/* Program headers: separate segments so we can set correct page permissions
 * (text executable, rodata read-only, data writable). */
PHDRS
{
    text            PT_LOAD FLAGS((1 << 0) | (1 << 2)); /* R + X */
    rodata          PT_LOAD FLAGS((1 << 2));            /* R     */
    data            PT_LOAD FLAGS((1 << 1) | (1 << 2)); /* R + W */
}

KERNEL_VMA = 0xffffffff80200000;

SECTIONS
{
    . = KERNEL_VMA;
    __kernel_start = .;

    .text : ALIGN(4K) {
        *(.text .text.*)
    } :text

    .rodata : ALIGN(4K) {
        *(.rodata .rodata.*)
    } :rodata

    /* Limine requests. Written by the bootloader (responses), so they live in a
     * writable segment. Must start on its own page — Limine rejects a segment
     * that shares a page with a differently-permissioned one (here the R-only
     * .rodata). KEEP so --gc-sections cannot drop them; the markers bound the
     * bootloader's scan. Order start -> requests -> end. */
    .limine_requests : ALIGN(4K) {
        KEEP(*(.limine_requests_start))
        KEEP(*(.limine_requests))
        KEEP(*(.limine_requests_end))
    } :data

    .data : ALIGN(4K) {
        *(.data .data.*)
    } :data

    .bss : ALIGN(4K) {
        __bss_start = .;
        *(.bss .bss.*)
        *(COMMON)
        __bss_end = .;
    } :data

    __kernel_end = .;

    /DISCARD/ : {
        *(.eh_frame)
        *(.eh_frame_hdr)
        *(.note .note.*)
        *(.comment)
    }
}
