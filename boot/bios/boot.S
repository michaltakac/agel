.code16
.section .text
.global _start
_start:
    cli
    xorw %ax, %ax
    movw %ax, %ds
    movw %ax, %es
    movw %ax, %ss
    movw $0x7c00, %sp
    movb %dl, boot_drive

    # Enable A20 through the fast gate.
    inb $0x92, %al
    orb $2, %al
    outb %al, $0x92

    # Ask the VBE BIOS for a 1024x768x32 linear framebuffer while BIOS calls
    # are still available. Failure is non-fatal: the serial recovery plane must
    # boot even on a machine with no graphics device. The mode-information
    # block remains at 0x7000 for the kernel; 0x6ff0 carries a success marker.
    movl $0, 0x6ff0
    xorw %ax, %ax
    movw %ax, %es
    movw $0x7000, %di
    movw $0x4f01, %ax
    # SeaBIOS/QEMU's 32-bit 1024x768 mode.
    movw $0x0144, %cx
    int $0x10
    cmpw $0x004f, %ax
    jne graphics_done
    testw $0x0080, 0x7000
    jz graphics_done
    cmpb $32, 0x7019
    jne graphics_done
    movw $0x4f02, %ax
    movw $0x4144, %bx
    int $0x10
    cmpw $0x004f, %ax
    jne graphics_done
    movl $0xa6e10fb0, 0x6ff0
graphics_done:

    # Kernel slot selection. Sector 1057 is the selector: "AGKS", version,
    # trusted slot, candidate slot (0xff none), boot attempts, verified flag,
    # admitted flag. A candidate the running kernel has not admitted (its
    # signature checked) is never loaded. An unverified candidate is charged
    # one attempt here, before any of it runs, and after three it is skipped
    # for the trusted slot. The chosen slot is left at 0x6fec behind a marker
    # at 0x6fe8 for the kernel.
    movw $disk_packet, %si
    movb boot_drive, %dl
    movb $0x42, %ah
    int $0x13
    jc halt
    xorw %bx, %bx
    cmpl $0x534b4741, 0x6000
    jne select_done
    movb 0x6005, %bl
    movb 0x6006, %al
    cmpb $0xff, %al
    je select_done
    cmpb $0, 0x6009
    je select_done
    cmpb $0, 0x6008
    jne select_candidate
    cmpb $3, 0x6007
    jae select_done
    incb 0x6007
    movw $0x4300, %ax
    int $0x13
    jc halt
    movb 0x6006, %al
select_candidate:
    movb %al, %bl
select_done:
    movb %bl, 0x6fec
    movl $0xa6e15107, 0x6fe8

    # Load the kernel to physical 0x100000, above the real-mode megabyte:
    # 33 conservative 127-sector BIOS transfers (each below the common
    # firmware limit) from the chosen slot into a bounce buffer at 0x10000,
    # each copied up with 32-bit addressing while the data segments carry a
    # 4 GiB limit (unreal mode: protected mode entered and left with the
    # limits kept). Slot A begins at sector 65536, slot B at 69760, past the
    # data region, so a 2 MiB kernel needs no filesystem to be found.
    lgdt gdt_pointer
    movl %cr0, %eax
    orl $1, %eax
    movl %eax, %cr0
    movw $0x10, %cx
    movw %cx, %ds
    movw %cx, %es
    decl %eax
    movl %eax, %cr0
    xorw %cx, %cx
    movw %cx, %ds
    movw %cx, %es
    movl $65536, %eax
    testb %bl, %bl
    jz base_done
    addw $4224, %ax
base_done:
    movl %eax, disk_packet + 8
    movw $127, disk_packet + 2
    # Offset 0, segment 0x1000: the bounce buffer at 0x10000.
    movl $0x10000000, disk_packet + 4
    movl $0x100000, %edi
    movw $33, %cx
load_loop:
    pushw %cx
    pushl %edi
    # The copy leaves esi past the buffer; the packet is at si again.
    movw $disk_packet, %si
    movb $0x42, %ah
    int $0x13
    jc halt
    popl %edi
    addl $127, disk_packet + 8
    movl $0x10000, %esi
    movl $16256, %ecx
    # addr32 rep movsl: esi/edi through the unreal data segments.
    .byte 0x67, 0xf3, 0x66, 0xa5
    popw %cx
    loop load_loop

    # Zero PML4, PDPT and PD at 0x1000..0x3fff.
    xorw %ax, %ax
    movw %ax, %es
    movw $0x1000, %di
    movw $0x1800, %cx
    rep stosw

    # Identity-map the first GiB with 2 MiB pages.
    movl $0x2003, 0x1000
    movl $0x3003, 0x2000
    movw $0x3000, %di
    xorl %eax, %eax
    movw $512, %cx
page_loop:
    movl %eax, %edx
    orl $0x83, %edx
    movl %edx, (%di)
    movl $0, 4(%di)
    addw $8, %di
    addl $0x200000, %eax
    loop page_loop

    lgdt gdt_pointer
    movl %cr4, %eax
    orl $1<<5, %eax
    movl %eax, %cr4
    movl $0x1000, %eax
    movl %eax, %cr3
    movl $0xc0000080, %ecx
    rdmsr
    orl $1<<8, %eax
    wrmsr
    movl %cr0, %eax
    orl $(1<<31)|1, %eax
    movl %eax, %cr0
    ljmpl $0x08, $long_mode

halt:
    cli
    hlt
    jmp halt

.code64
long_mode:
    movw $0x10, %ax
    movw %ax, %ds
    movw %ax, %es
    movw %ax, %ss
    # The supervisor stack: a megabyte, from 5 MiB down to 4 MiB, above the
    # kernel image at 1 MiB (2 MiB of slot, then its zeroed data to 4 MiB)
    # and below the frame pool at 6 MiB; nothing else lives there. It was
    # 0x90000 in low memory, some 200 KiB, and one command started from
    # inside another overflowed it.
    movq $0x500000, %rsp
    call 0x100000
    jmp halt

.code16
boot_drive: .byte 0

.align 8
gdt:
    .quad 0
    .quad 0x00af9a000000ffff
    .quad 0x00cf92000000ffff
gdt_pointer:
    .word . - gdt - 1
    .long gdt

.align 4
disk_packet:
    .byte 0x10, 0
    .word 1
    .word 0x6000
    .word 0
    .quad 1057

.org 510
.word 0xaa55
