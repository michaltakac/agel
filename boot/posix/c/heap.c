/* The heap grows: more memory than the first pages of the heap, blocks
   freed and reused, an impossible request refused; then the working
   directory, and a file truncated and grown with zeros. Run at the
   namespace's root with the writer's files present. */
#include <errno.h>
#include <fcntl.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>
#include <unistd.h>

#define CHUNK (64 * 1024)
#define CHUNKS 16

int main(void) {
    char *chunks[CHUNKS];
    for (int index = 0; index < CHUNKS; index++) {
        chunks[index] = malloc(CHUNK);
        if (chunks[index] == NULL) {
            printf("heap: malloc %d: errno %d\n", index, errno);
            return 1;
        }
        memset(chunks[index], 'a' + index, CHUNK);
    }
    for (int index = 0; index < CHUNKS; index++) {
        for (int at = 0; at < CHUNK; at += 4093) {
            if (chunks[index][at] != 'a' + index) {
                printf("heap: FAILED: chunk %d corrupt\n", index);
                return 1;
            }
        }
    }
    printf("heap: %d chunks of 64 KiB filled and checked\n", CHUNKS);
    for (int index = 0; index < CHUNKS; index++) {
        free(chunks[index]);
    }
    char *big = malloc(1024 * 1024);
    if (big == NULL) {
        printf("heap: malloc 1 MiB: errno %d\n", errno);
        return 1;
    }
    big[0] = 1;
    big[1024 * 1024 - 1] = 2;
    printf("heap: one block of 1 MiB, %s\n", big[0] + big[1024 * 1024 - 1] == 3 ? "written" : "lost");
    free(big);
    char *absurd = malloc(64 * 1024 * 1024);
    printf("heap: 64 MiB refused: errno %d\n", absurd == NULL ? errno : 0);
    void *edge = sbrk(0);
    printf("heap: the break is %s\n", edge != (void *)-1 ? "known" : "unknown");

    char cwd[64];
    if (chdir("app") != 0) {
        printf("heap: chdir app: errno %d\n", errno);
        return 1;
    }
    getcwd(cwd, sizeof cwd);
    int notes = open("notes", O_RDONLY);
    char text[32];
    ssize_t count = notes < 0 ? -1 : read(notes, text, sizeof text);
    close(notes);
    printf("heap: cwd %s, notes has %ld bytes\n", cwd, (long)count);
    chdir("../");
    getcwd(cwd, sizeof cwd);
    printf("heap: cwd %s, chdir nowhere: errno %d\n", cwd, chdir("nowhere") == 0 ? 0 : errno);

    int file = open("app/trunc", O_RDWR | O_CREAT);
    if (file < 0) {
        printf("heap: open trunc: errno %d\n", errno);
        return 1;
    }
    char hundred[100];
    memset(hundred, 'x', sizeof hundred);
    write(file, hundred, sizeof hundred);
    struct stat info;
    ftruncate(file, 10);
    stat("app/trunc", &info);
    long after_cut = (long)info.st_size;
    ftruncate(file, 20);
    stat("app/trunc", &info);
    long after_grow = (long)info.st_size;
    lseek(file, 0, SEEK_SET);
    char back[32];
    ssize_t got = read(file, back, sizeof back);
    int zeros = 1;
    for (int at = 10; at < 20 && at < got; at++) {
        if (back[at] != 0) {
            zeros = 0;
        }
    }
    close(file);
    printf("heap: truncated to %ld, grown to %ld, read %ld bytes, the new ones %s\n", after_cut,
           after_grow, (long)got, zeros && got == 20 ? "zero" : "not zero");
    truncate("app/trunc", 0);
    stat("app/trunc", &info);
    printf("heap: truncate to 0 leaves %ld bytes\n", (long)info.st_size);
    unlink("app/trunc");
    return 0;
}
