ENTRY(agel_boot)

SECTIONS
{
  . = 0x10000;
  .text : ALIGN(4K) { KEEP(*(.text.entry)) *(.text .text.*) }

  /* Ring-3 program text. It is the only part of the kernel image the page
     tables mark user-executable, so it is isolated in its own page-aligned
     output section and the boundary symbols are exported to the kernel. */
  .user_text : ALIGN(4K)
  {
    __user_text_start = .;
    KEEP(*(.user_text))
    . = ALIGN(4K);
    __user_text_end = .;
  }

  .rodata : ALIGN(4K) { *(.rodata .rodata.*) }
  .data : ALIGN(4K) { *(.data .data.*) }
  .bss : ALIGN(4K)
  {
    __bss_start = .;
    *(COMMON) *(.bss .bss.*)
    . = ALIGN(8);
    __bss_end = .;
  }
  /DISCARD/ : { *(.eh_frame*) *(.comment*) }
}
