/* agel-libc: a bump-allocated heap and exit. */
#ifndef AGEL_STDLIB_H
#define AGEL_STDLIB_H
#include <stddef.h>

void *malloc(size_t size);
void *calloc(size_t count, size_t size);
void *realloc(void *pointer, size_t size);
void free(void *pointer);
void exit(int status) __attribute__((noreturn));
void abort(void) __attribute__((noreturn));

#endif
