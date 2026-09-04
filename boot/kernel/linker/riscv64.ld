ENTRY(agel_boot)

/* OpenSBI occupies the first 2 MiB of RAM on QEMU's `virt` machine and hands
   S-mode control to 0x80200000. */
SECTIONS
{
  . = 0x80200000;
  .entry : ALIGN(4K) { KEEP(*(.text.entry)) }

  /* Unprivileged program text. It is the only part of the kernel image the
     page tables mark U-mode-executable, so it is isolated in its own
     page-aligned output section and the boundary symbols are exported. */
  .user_text : ALIGN(4K)
  {
    __user_text_start = .;
    *(.text.*native*)
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
    *(.rodata .rodata.*) *(.srodata .srodata.*) *(.got .got.*)
    . = ALIGN(4K);
    __rodata_end = .;
  }
  .data : ALIGN(4K) { *(.data .data.*) *(.sdata .sdata.*) }
  .bss : ALIGN(4K)
  {
    __bss_start = .;
    *(COMMON) *(.bss .bss.*) *(.sbss .sbss.*)
    . = ALIGN(8);
    __bss_end = .;
  }

  . = ALIGN(4K);
  __stack_bottom = .;
  . += 0x10000;
  __stack_top = .;

  /DISCARD/ : { *(.eh_frame*) *(.comment*) }
}
