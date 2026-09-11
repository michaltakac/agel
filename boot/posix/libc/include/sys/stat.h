/* agel-libc: what the filesystem service knows about a name, which is a
   kind and a length; the mode bits are a fixed 0644 with the kind. */
#ifndef AGEL_SYS_STAT_H
#define AGEL_SYS_STAT_H
#include <sys/types.h>

struct stat {
    off_t st_size;
    unsigned int st_mode;
};

#define S_IFMT 0170000
#define S_IFREG 0100000
#define S_IFDIR 0040000
#define S_ISREG(mode) (((mode) & S_IFMT) == S_IFREG)
#define S_ISDIR(mode) (((mode) & S_IFMT) == S_IFDIR)

int stat(const char *path, struct stat *buffer);
/* The mode is accepted and ignored: the service has no modes. */
int mkdir(const char *path, unsigned int mode);

#endif
