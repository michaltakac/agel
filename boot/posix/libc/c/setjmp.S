/* agel-libc: setjmp saves the callee-saved registers, the stack pointer
   and the return address into the buffer and answers 0; longjmp restores
   them and returns from setjmp again with the value (1 for 0). */
#if defined(__x86_64__)
    .text
    .globl setjmp
    .type setjmp, @function
setjmp:
    movq %rbx, 0(%rdi)
    movq %rbp, 8(%rdi)
    movq %r12, 16(%rdi)
    movq %r13, 24(%rdi)
    movq %r14, 32(%rdi)
    movq %r15, 40(%rdi)
    leaq 8(%rsp), %rax
    movq %rax, 48(%rdi)
    movq (%rsp), %rax
    movq %rax, 56(%rdi)
    xorl %eax, %eax
    ret
    .globl longjmp
    .type longjmp, @function
longjmp:
    movl %esi, %eax
    testl %eax, %eax
    jnz 1f
    movl $1, %eax
1:
    movq 0(%rdi), %rbx
    movq 8(%rdi), %rbp
    movq 16(%rdi), %r12
    movq 24(%rdi), %r13
    movq 32(%rdi), %r14
    movq 40(%rdi), %r15
    movq 48(%rdi), %rsp
    jmpq *56(%rdi)
#elif defined(__aarch64__)
    .text
    .globl setjmp
    .type setjmp, %function
setjmp:
    stp x19, x20, [x0, #0]
    stp x21, x22, [x0, #16]
    stp x23, x24, [x0, #32]
    stp x25, x26, [x0, #48]
    stp x27, x28, [x0, #64]
    stp x29, x30, [x0, #80]
    mov x2, sp
    str x2, [x0, #96]
    mov w0, #0
    ret
    .globl longjmp
    .type longjmp, %function
longjmp:
    ldp x19, x20, [x0, #0]
    ldp x21, x22, [x0, #16]
    ldp x23, x24, [x0, #32]
    ldp x25, x26, [x0, #48]
    ldp x27, x28, [x0, #64]
    ldp x29, x30, [x0, #80]
    ldr x2, [x0, #96]
    mov sp, x2
    cmp w1, #0
    csinc w0, w1, wzr, ne
    ret
#elif defined(__riscv)
    .text
    .globl setjmp
    .type setjmp, @function
setjmp:
    sd ra, 0(a0)
    sd sp, 8(a0)
    sd s0, 16(a0)
    sd s1, 24(a0)
    sd s2, 32(a0)
    sd s3, 40(a0)
    sd s4, 48(a0)
    sd s5, 56(a0)
    sd s6, 64(a0)
    sd s7, 72(a0)
    sd s8, 80(a0)
    sd s9, 88(a0)
    sd s10, 96(a0)
    sd s11, 104(a0)
    li a0, 0
    ret
    .globl longjmp
    .type longjmp, @function
longjmp:
    ld ra, 0(a0)
    ld sp, 8(a0)
    ld s0, 16(a0)
    ld s1, 24(a0)
    ld s2, 32(a0)
    ld s3, 40(a0)
    ld s4, 48(a0)
    ld s5, 56(a0)
    ld s6, 64(a0)
    ld s7, 72(a0)
    ld s8, 80(a0)
    ld s9, 88(a0)
    ld s10, 96(a0)
    ld s11, 104(a0)
    seqz a0, a1
    add a0, a0, a1
    ret
#endif
    .section .note.GNU-stack,"",%progbits
