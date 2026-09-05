ENTRY(agel_boot)

SECTIONS
{
  . = 0x10000;
  .entry : ALIGN(4K) { KEEP(*(.text.entry)) }

  /* Ring-3 program text. It is the only part of the kernel image the page
     tables mark user-executable, so it is isolated in its own page-aligned
     output section and the boundary symbols are exported to the kernel. */
  .user_text : ALIGN(4K)
  {
    __user_text_start = .;
    *(.text.*native*)
    *(.text.*display_user*)
    *(.text.mem*)
    *(.text.*compiler_builtins*mem*)
    *(.text.*slice_index_order_fail*)
    *(.text.*panic_bounds_check*)
    KEEP(*(.user_text))
    . = ALIGN(4K);
    __user_text_end = .;
  }

  .text : ALIGN(4K) { *(.text .text.*) }
  .rodata : ALIGN(4K)
  {
    __rodata_start = .;
    *(.rodata .rodata.*) *(.got .got.*)
    . = ALIGN(4K);
    __rodata_end = .;
  }
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
