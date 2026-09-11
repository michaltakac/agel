/* Names in the namespace, from C: mkdir, stat, rename, a directory read
   with opendir and readdir, unlink and rmdir; then sscanf and getopt on
   the arguments. Run in a namespace rooted at a directory holding the
   writer's notes: ":exec c-dir /app -- -v -o out.txt notes". */
#include <dirent.h>
#include <errno.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>
#include <unistd.h>

static void listing(const char *path) {
    DIR *directory = opendir(path);
    if (directory == NULL) {
        printf("dir: opendir %s: errno %d\n", path, errno);
        return;
    }
    printf("dir: %s:", path);
    struct dirent *entry;
    while ((entry = readdir(directory)) != NULL) {
        printf(" %s%s", entry->d_name, entry->d_type == DT_DIR ? "/" : "");
    }
    printf("\n");
    closedir(directory);
}

int main(int argc, char **argv) {
    int verbose = 0;
    const char *output = "none";
    int option;
    while ((option = getopt(argc, argv, "vo:")) != -1) {
        if (option == 'v') {
            verbose = 1;
        } else if (option == 'o') {
            output = optarg;
        } else {
            printf("dir: usage: c-dir [-v] [-o FILE] NAME\n");
            return 2;
        }
    }
    const char *name = optind < argc ? argv[optind] : "notes";
    printf("dir: verbose %d, output %s, name %s\n", verbose, output, name);

    struct stat info;
    if (stat(name, &info) != 0) {
        printf("dir: stat %s: errno %d\n", name, errno);
        return 1;
    }
    printf("dir: %s is a %s of %ld bytes\n", name, S_ISREG(info.st_mode) ? "file" : "directory",
           (long)info.st_size);
    if (mkdir("logs", 0755) != 0) {
        printf("dir: mkdir logs: errno %d\n", errno);
        return 1;
    }
    if (stat("logs", &info) == 0 && S_ISDIR(info.st_mode)) {
        printf("dir: logs is a directory\n");
    }
    if (rename(name, "logs/kept") != 0) {
        printf("dir: rename: errno %d\n", errno);
        return 1;
    }
    listing(".");
    listing("logs");
    printf("dir: rmdir logs while full: errno %d\n", rmdir("logs") == 0 ? 0 : errno);
    if (unlink("logs/kept") != 0 || rmdir("logs") != 0) {
        printf("dir: unlink or rmdir: errno %d\n", errno);
        return 1;
    }
    printf("dir: stat after unlink: errno %d\n", stat(name, &info) == 0 ? 0 : errno);
    printf("dir: rename of nothing: errno %d\n", rename("nowhere", "logs/x") == 0 ? 0 : errno);

    int number = 0;
    unsigned hex = 0;
    char word[16] = "";
    char letter = 0;
    int fields = sscanf("  42 ff hello x", "%d %x %15s %c", &number, &hex, word, &letter);
    printf("dir: sscanf %d fields: %d %u %s %c\n", fields, number, hex, word, letter);
    return 0;
}
