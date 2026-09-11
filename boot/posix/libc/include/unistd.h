/* agel-libc: process I/O through the Agel process protocol. */
#ifndef AGEL_UNISTD_H
#define AGEL_UNISTD_H
#include <stddef.h>
#include <sys/types.h>

#define STDIN_FILENO 0
#define STDOUT_FILENO 1
#define STDERR_FILENO 2

ssize_t read(int descriptor, void *buffer, size_t count);
int pipe(int descriptors[2]);
ssize_t write(int descriptor, const void *buffer, size_t count);
int close(int descriptor);
void _exit(int status) __attribute__((noreturn));

int unlink(const char *path);
int rmdir(const char *path);

/* getopt, in the library's C: optind, optarg, opterr and optopt as usual;
   an option string of letters, a colon after one that takes an argument;
   "--" ends the options; an unknown or argument-less option prints to
   stderr unless opterr is zero and answers '?'. */
extern int optind, opterr, optopt;
extern char *optarg;
int getopt(int argc, char *const argv[], const char *options);

#define SEEK_SET 0
#define SEEK_CUR 1
#define SEEK_END 2
off_t lseek(int descriptor, off_t offset, int whence);

#endif
