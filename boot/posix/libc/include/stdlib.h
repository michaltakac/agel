/* agel-libc: a free-list heap, numbers, sorting and exit. */
#ifndef AGEL_STDLIB_H
#define AGEL_STDLIB_H
#include <stddef.h>

#define EXIT_SUCCESS 0
#define EXIT_FAILURE 1

void *malloc(size_t size);
void *calloc(size_t count, size_t size);
void *realloc(void *pointer, size_t size);
void free(void *pointer);
void exit(int status) __attribute__((noreturn));
void abort(void) __attribute__((noreturn));
int atoi(const char *text);
long atol(const char *text);
long strtol(const char *text, char **end, int base);
unsigned long strtoul(const char *text, char **end, int base);
int abs(int value);
long labs(long value);
void qsort(void *base, size_t count, size_t size, int (*compare)(const void *, const void *));

#endif
