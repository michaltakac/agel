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

    # Load the kernel in two conservative 127-sector BIOS transfers. One DAP
    # request is deliberately kept below the common firmware limit; together
    # they leave 127 KiB for the isolated evaluator without making the seed
    # depend on a filesystem.
    movw $disk_packet, %si
    movb boot_drive, %dl
    movb $0x42, %ah
    int $0x13
    jc disk_error
    movw $0x1fe0, disk_packet + 6
    movl $128, disk_packet + 8
    movb boot_drive, %dl
    movb $0x42, %ah
    int $0x13
    jc disk_error

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

disk_error:
    movw $disk_error_message, %si
print:
    lodsb
    testb %al, %al
    jz halt
    movb $0x0e, %ah
    int $0x10
    jmp print
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
    movq $0x90000, %rsp
    xorq %rbp, %rbp
    movq $0x10000, %rax
    call *%rax
    jmp halt

.code16
boot_drive: .byte 0
disk_error_message: .asciz "Agel disk read failed"

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
    .word 127
    .word 0
    .word 0x1000
    .quad 1

.org 510
.word 0xaa55
