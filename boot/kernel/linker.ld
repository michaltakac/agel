ENTRY(agel_boot)

SECTIONS
{
  . = 0x10000;
  .text : ALIGN(4K) { KEEP(*(.text.entry)) *(.text .text.*) }
  .rodata : ALIGN(4K) { *(.rodata .rodata.*) }
  .data : ALIGN(4K) { *(.data .data.*) }
  .bss : ALIGN(4K) { *(COMMON) *(.bss .bss.*) }
  /DISCARD/ : { *(.eh_frame*) *(.comment*) }
}
