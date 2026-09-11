/* agel-libc: console output. printf formats %s %d %i %u %x %c %p %% with
   l and z length modifiers; there are no streams and no input yet. */
#ifndef AGEL_STDIO_H
#define AGEL_STDIO_H

int printf(const char *format, ...);
int puts(const char *text);
int putchar(int character);

#endif
