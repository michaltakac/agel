/* agel-libc: the error numbers the process protocol answers. */
#ifndef AGEL_ERRNO_H
#define AGEL_ERRNO_H

int *__errno_location(void);
#define errno (*__errno_location())

#define ENOENT 2
#define ESRCH 3
#define EIO 5
#define EBADF 9
#define ECHILD 10
#define EAGAIN 11
#define ENOMEM 12
#define EACCES 13
#define EBUSY 16
#define EEXIST 17
#define ENODEV 19
#define ENOTDIR 20
#define EISDIR 21
#define EINVAL 22
#define ENFILE 23
#define EMFILE 24
#define EFBIG 27
#define ENOSPC 28
#define ERANGE 34
#define ESPIPE 29
#define EPIPE 32
#define ENOSYS 38
#define ENOTEMPTY 39
#define ESTALE 116

#endif
