/* agel-libc: memory and string routines. */
#ifndef AGEL_STRING_H
#define AGEL_STRING_H
#include <stddef.h>

void *memcpy(void *destination, const void *source, size_t count);
void *memmove(void *destination, const void *source, size_t count);
void *memset(void *destination, int value, size_t count);
int memcmp(const void *left, const void *right, size_t count);
void *memchr(const void *block, int wanted, size_t count);
size_t strlen(const char *text);
int strcmp(const char *left, const char *right);
int strncmp(const char *left, const char *right, size_t count);
char *strcpy(char *destination, const char *source);
char *strncpy(char *destination, const char *source, size_t count);
char *strcat(char *destination, const char *source);
char *strncat(char *destination, const char *source, size_t count);
char *strchr(const char *text, int wanted);
char *strrchr(const char *text, int wanted);
char *strstr(const char *haystack, const char *needle);
char *strdup(const char *text);
char *strerror(int number);

#endif
