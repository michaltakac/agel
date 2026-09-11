/* Files beyond one block: fifty thousand bytes written and read back,
   cut and grown with zeros, the region filled to ENOSPC with files of
   64 KiB, everything removed, and space seen to come back. Run at the
   namespace's root with the writer's two files present. */
#include <errno.h>
#include <fcntl.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>
#include <unistd.h>

#define BIG 50000
#define FULL (64 * 1024)

static int write_all(int file, const char *bytes, size_t count) {
    size_t done = 0;
    while (done < count) {
        ssize_t got = write(file, bytes + done, count - done);
        if (got <= 0) {
            return -1;
        }
        done += (size_t)got;
    }
    return 0;
}

int main(void) {
    char *buffer = malloc(FULL);
    if (buffer == NULL) {
        printf("big: malloc: errno %d\n", errno);
        return 1;
    }
    for (int at = 0; at < BIG; at++) {
        buffer[at] = (char)(at % 251);
    }
    int file = open("app/big", O_RDWR | O_CREAT);
    if (file < 0 || write_all(file, buffer, BIG) != 0) {
        printf("big: write: errno %d\n", errno);
        return 1;
    }
    struct stat info;
    stat("app/big", &info);
    lseek(file, 0, SEEK_SET);
    memset(buffer, 0, FULL);
    size_t got = 0;
    for (;;) {
        ssize_t count = read(file, buffer + got, FULL - got);
        if (count <= 0) {
            break;
        }
        got += (size_t)count;
    }
    int intact = got == BIG;
    for (int at = 0; at < BIG && intact; at++) {
        if (buffer[at] != (char)(at % 251)) {
            intact = 0;
        }
    }
    printf("big: %ld bytes on disk, %zu read back, %s\n", (long)info.st_size, got,
           intact ? "intact" : "CORRUPT");
    ftruncate(file, 3000);
    ftruncate(file, 10000);
    lseek(file, 0, SEEK_SET);
    got = 0;
    for (;;) {
        ssize_t count = read(file, buffer + got, FULL - got);
        if (count <= 0) {
            break;
        }
        got += (size_t)count;
    }
    int zeros = got == 10000;
    for (size_t at = 3000; at < got && zeros; at++) {
        if (buffer[at] != 0) {
            zeros = 0;
        }
    }
    int kept = 1;
    for (int at = 0; at < 3000 && kept; at++) {
        if (buffer[at] != (char)(at % 251)) {
            kept = 0;
        }
    }
    printf("big: cut to 3000 and grown to 10000: %s, %s\n", kept ? "kept" : "LOST",
           zeros ? "the growth zero" : "the growth NOT zero");
    lseek(file, FULL, SEEK_SET);
    printf("big: a byte past 64 KiB: errno %d\n", write(file, "x", 1) < 0 ? errno : 0);
    close(file);

    memset(buffer, 'f', FULL);
    int full_files = 0;
    int last_error = 0;
    char name[32];
    for (int index = 0; index < 8; index++) {
        snprintf(name, sizeof name, "app/fill%d", index);
        int fill = open(name, O_WRONLY | O_CREAT);
        if (fill < 0) {
            last_error = errno;
            break;
        }
        if (write_all(fill, buffer, FULL) != 0) {
            last_error = errno;
            close(fill);
            break;
        }
        close(fill);
        full_files++;
    }
    printf("big: %d files of 64 KiB, then errno %d\n", full_files, last_error);
    for (int index = 0; index <= full_files; index++) {
        snprintf(name, sizeof name, "app/fill%d", index);
        unlink(name);
    }
    unlink("app/big");
    int again = open("app/again", O_WRONLY | O_CREAT);
    int back = again >= 0 && write_all(again, buffer, FULL) == 0;
    close(again);
    unlink("app/again");
    printf("big: space came back: %s\n", back ? "yes" : "no");
    return 0;
}
