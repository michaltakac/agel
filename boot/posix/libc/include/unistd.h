/* agel-libc: process I/O through the Agel process protocol. */
#ifndef AGEL_UNISTD_H
#define AGEL_UNISTD_H
#include <stddef.h>

typedef long ssize_t;

#define STDIN_FILENO 0
#define STDOUT_FILENO 1
#define STDERR_FILENO 2

typedef int pid_t;

ssize_t read(int descriptor, void *buffer, size_t count);
int pipe(int descriptors[2]);
ssize_t write(int descriptor, const void *buffer, size_t count);
int close(int descriptor);
void _exit(int status) __attribute__((noreturn));

#endif
