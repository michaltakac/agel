/* agel-libc: setjmp and longjmp, in assembly for each machine; the buffer
   holds the callee-saved registers, the stack pointer and the return
   address. */
#ifndef AGEL_SETJMP_H
#define AGEL_SETJMP_H

typedef unsigned long jmp_buf[16];

int setjmp(jmp_buf buffer) __attribute__((returns_twice));
void longjmp(jmp_buf buffer, int value) __attribute__((noreturn));

#endif
