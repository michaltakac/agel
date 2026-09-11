ENTRY(agel_boot)

/* QEMU's `virt` machine places RAM at 1 GiB and loads an ELF kernel by its
   program headers, so the image simply states where it wants to live. A
   board whose firmware loads a flat image elsewhere (the Raspberry Pi, at
   0x80000) says so through AGEL_LOAD, which the build script defines. */
SECTIONS
{
  . = DEFINED(AGEL_LOAD) ? AGEL_LOAD : 0x40080000;
  .entry : ALIGN(4K) { KEEP(*(.text.entry)) }

  /* VBAR_EL1 ignores the low eleven bits of the vector base, so the table gets
     its own output section with the alignment the architecture requires rather
     than the alignment the assembler happens to pick. */
  .vectors : ALIGN(2K) { KEEP(*(.vectors)) }

  /* Unprivileged program text. It is the only part of the kernel image the
     page tables mark EL0-executable, so it is isolated in its own page-aligned
     output section and the boundary symbols are exported to the kernel. */
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

  /* The supervisor stack is part of the image's memory range rather than a
     guess about where RAM is free. */
  . = ALIGN(4K);
  __stack_bottom = .;
  /* 512 KiB, like x86-64: the workshop keeps two bounded source workspaces
     and a sector image on this stack rather than in an allocator. */
  . += 0x80000;
  __stack_top = .;

  /DISCARD/ : { *(.eh_frame*) *(.comment*) }
}
