OUTPUT_FORMAT(elf64-x86-64)
OUTPUT_ARCH(i386:x86-64)
ENTRY(kmain)

PHDRS
{
    limine_requests PT_LOAD;
    text PT_LOAD FLAGS((1 << 0) | (1 << 2));
    rodata PT_LOAD FLAGS((1 << 2));
    data PT_LOAD FLAGS((1 << 1) | (1 << 2));
}

SECTIONS
{
    . = 0xffffffff80000000;

    .limine_requests : {
        KEEP(*(.limine_requests_start))
        KEEP(*(.limine_requests))
        KEEP(*(.limine_requests_end))
    } :limine_requests

    . = ALIGN(CONSTANT(MAXPAGESIZE));

    .text :
    {
        *(.text .text.*)
    } :text
 
    . = ALIGN(CONSTANT(MAXPAGESIZE));

    .rodata : 
    {
        *(.rodata .rodata.*)
    } :rodata
 
    . = ALIGN(CONSTANT(MAXPAGESIZE));

    .data : 
    {
        *(.data .data.*)
    } :data
 
    .bss : 
    {
        *(COMMON)
        *(.bss .bss.*)
    } :data

    /DISCARD/ : {
        *(.got)
        *(.comment)
        *(.eh_frame*)
        *(.note .note.*)
    }
}

