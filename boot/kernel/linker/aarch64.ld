ENTRY(agel_boot)

/* QEMU's `virt` machine places RAM at 1 GiB and loads an ELF kernel by its
   program headers, so the image simply states where it wants to live. */
SECTIONS
{
  . = 0x40080000;
  .text : ALIGN(4K) { KEEP(*(.text.entry)) *(.text .text.*) }

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

  /* The supervisor stack is part of the image's memory range rather than a
     guess about where RAM is free. */
  . = ALIGN(4K);
  __stack_bottom = .;
  . += 0x10000;
  __stack_top = .;

  /DISCARD/ : { *(.eh_frame*) *(.comment*) }
}
