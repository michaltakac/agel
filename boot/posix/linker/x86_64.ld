/* A process image for the Agel x86_64 research kernel: a static executable
   placed in the process window the supervisor maps it into. Segments are
   page aligned so that no page is both writable and executable. */
ENTRY(_start)
SECTIONS
{
  . = 0x0000008010000000;
  .text : ALIGN(4K) { *(.text.entry) *(.text .text.*) }
  .rodata : ALIGN(4K) { *(.rodata .rodata.*) }
  /* Position-independent C keeps its GOT and relocated constants here, in
     the writable segment, so no section is left for the linker to place on
     a page of another segment. */
  .data : ALIGN(4K) { *(.data .data.*) *(.sdata .sdata.*) *(.got .got.*) *(.data.rel.ro .data.rel.ro.*) }
  .bss : ALIGN(4K) { *(COMMON) *(.bss .bss.*) *(.sbss .sbss.*) }
  /DISCARD/ : { *(.eh_frame*) *(.comment*) *(.note*) }
}
