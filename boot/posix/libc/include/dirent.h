/* agel-libc: reading a directory through a descriptor opened on it. Four
   directories may be open at once. */
#ifndef AGEL_DIRENT_H
#define AGEL_DIRENT_H

#define DT_DIR 4
#define DT_REG 8

struct dirent {
    unsigned int d_type;
    char d_name[33];
};

typedef struct __agel_dir DIR;

DIR *opendir(const char *path);
struct dirent *readdir(DIR *directory);
int closedir(DIR *directory);

#endif
