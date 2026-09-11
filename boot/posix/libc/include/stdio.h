/* agel-libc: streams over descriptors, and formatting. The formatter
   handles %s %c %d %i %u %x %X %o %p %% with the flags - 0 + and space, a
   width, a precision, and the l, ll, z and h modifiers; no floating point.
   Streams are line buffered for output and block buffered for input. */
#ifndef AGEL_STDIO_H
#define AGEL_STDIO_H
#include <stdarg.h>
#include <stddef.h>

#define EOF (-1)
#define BUFSIZ 512

typedef struct __agel_file FILE;
extern FILE *stdin;
extern FILE *stdout;
extern FILE *stderr;

FILE *fopen(const char *path, const char *mode);
int fclose(FILE *stream);
int fflush(FILE *stream);
int fileno(FILE *stream);
int feof(FILE *stream);
int ferror(FILE *stream);
void clearerr(FILE *stream);

int fputc(int character, FILE *stream);
int putc(int character, FILE *stream);
int putchar(int character);
int fputs(const char *text, FILE *stream);
int puts(const char *text);
size_t fwrite(const void *data, size_t size, size_t count, FILE *stream);

int fgetc(FILE *stream);
int ungetc(int character, FILE *stream);
int getc(FILE *stream);
int getchar(void);
char *fgets(char *buffer, int size, FILE *stream);
size_t fread(void *data, size_t size, size_t count, FILE *stream);

int printf(const char *format, ...);
int fprintf(FILE *stream, const char *format, ...);
int sprintf(char *buffer, const char *format, ...);
int snprintf(char *buffer, size_t capacity, const char *format, ...);
int vprintf(const char *format, va_list arguments);
int vfprintf(FILE *stream, const char *format, va_list arguments);
int vsprintf(char *buffer, const char *format, va_list arguments);
int vsnprintf(char *buffer, size_t capacity, const char *format, va_list arguments);
void perror(const char *prefix);
int rename(const char *old, const char *new);

/* The scanner handles %d %i %u %x %X %o %s %c %% with a width and the
   l, ll and h modifiers, whitespace in the format matching any amount of
   input whitespace; no floating point, no %[. */
int vsscanf(const char *text, const char *format, va_list arguments);
int sscanf(const char *text, const char *format, ...);
int vfscanf(FILE *stream, const char *format, va_list arguments);
int fscanf(FILE *stream, const char *format, ...);
int scanf(const char *format, ...);

#endif
