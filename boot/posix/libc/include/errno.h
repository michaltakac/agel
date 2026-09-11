/* agel-libc: the error numbers the process protocol answers. */
#ifndef AGEL_ERRNO_H
#define AGEL_ERRNO_H

int *__errno_location(void);
#define errno (*__errno_location())

#define ENOENT 2
#define EIO 5
#define EBADF 9
#define ENOMEM 12
#define EACCES 13
#define ENOTDIR 20
#define EISDIR 21
#define EINVAL 22
#define EMFILE 24
#define EFBIG 27
#define ENOSPC 28
#define ENOSYS 38
#define ESTALE 116

#endif
