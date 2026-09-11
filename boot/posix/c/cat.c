/* cat: copies the named files, or `notes` when none is named, to the
   console, and reports the error number when the namespace has no such
   name. */
#include <errno.h>
#include <fcntl.h>
#include <stdio.h>
#include <unistd.h>

static int copy(const char *name) {
    int descriptor = open(name, O_RDONLY);
    if (descriptor < 0) {
        printf("cat: %s: errno %d\n", name, errno);
        return errno;
    }
    char buffer[64];
    for (;;) {
        ssize_t count = read(descriptor, buffer, sizeof buffer);
        if (count < 0) {
            printf("cat: read: errno %d\n", errno);
            return errno;
        }
        if (count == 0) {
            break;
        }
        write(1, buffer, (size_t)count);
    }
    close(descriptor);
    return 0;
}

int main(int argc, char **argv) {
    if (argc < 2) {
        return copy("notes");
    }
    for (int index = 1; index < argc; index++) {
        int status = copy(argv[index]);
        if (status != 0) {
            return status;
        }
    }
    return 0;
}
