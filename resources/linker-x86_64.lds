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
    
    _marker_kernel_start = .;
    _marker_limine_request_start = .;

    .limine_requests : {
        KEEP(*(.limine_requests_start))
        KEEP(*(.limine_requests))
        KEEP(*(.limine_requests_end))
    } :limine_requests

    _marker_limine_request_end = .;

    . = ALIGN(CONSTANT(MAXPAGESIZE));

    _marker_text_start = .;

    .text :
    {
        *(.text .text.*)
    } :text
    
    _marker_text_end = .;
 
    . = ALIGN(CONSTANT(MAXPAGESIZE));

    _marker_rodata_start = .;

    .rodata : 
    {
        *(.rodata .rodata.*)
    } :rodata

    _marker_got_start = .;
    .got :
    {
        *(.got)
    } :rodata
    _marker_got_end = .;

    _marker_rodata_end = .;
 
    . = ALIGN(CONSTANT(MAXPAGESIZE));

    _marker_data_start = .;

    .data : 
    {
        *(.data .data.*)
    } :data
 
    .bss : 
    {
        *(COMMON)
        *(.bss .bss.*)
    } :data

    _marker_data_end = .;
    _marker_kernel_end = .;

    /DISCARD/ : {
        *(.comment)
        *(.eh_frame*)
        *(.note .note.*)
    }
}

