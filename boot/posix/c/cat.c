/* cat for a namespace with one file in it: copies `notes` to the console
   and reports the error number when the namespace has no such name. */
#include <errno.h>
#include <fcntl.h>
#include <stdio.h>
#include <unistd.h>

int main(void) {
    int descriptor = open("notes", O_RDONLY);
    if (descriptor < 0) {
        printf("cat: notes: errno %d\n", errno);
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
