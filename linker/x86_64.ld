/* zeroxos x86_64 kernel linker script.
 *
 * The kernel is a higher-half kernel: it is linked to run at a virtual address
 * in the top 2 GiB of the 64-bit address space (matching `code-model: kernel`).
 * The load address 0xffffffff80200000 leaves the first 2 MiB below it free for
 * the multiboot2 header and early boot trampoline (added in M7).
 */

OUTPUT_FORMAT(elf64-x86-64)
OUTPUT_ARCH(i386:x86-64)
ENTRY(_start)

/* Program headers: separate segments so we can set correct page permissions
 * (text executable, rodata read-only, data writable) once paging is on. */
PHDRS
{
    text    PT_LOAD FLAGS((1 << 0) | (1 << 2)); /* R + X */
    rodata  PT_LOAD FLAGS((1 << 2));            /* R     */
    data    PT_LOAD FLAGS((1 << 1) | (1 << 2)); /* R + W */
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
