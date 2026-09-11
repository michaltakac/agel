/* agel-libc: open, through the namespace the process was given. */
#ifndef AGEL_FCNTL_H
#define AGEL_FCNTL_H

#define O_RDONLY 0
#define O_WRONLY 01
#define O_RDWR 02
#define O_CREAT 0100
#define O_DIRECTORY 0200000

int open(const char *path, int flags, ...);

#endif
