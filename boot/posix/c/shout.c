/* A child for a pipeline: reads its standard input to the end, uppercases
   it, writes it to its standard output, and exits with how many bytes it
   read. Its standard input is whatever its parent chose to give it. */
#include <unistd.h>

int main(void) {
    char buffer[64];
    int total = 0;
    for (;;) {
        ssize_t count = read(0, buffer, sizeof buffer);
        if (count <= 0) {
            break;
        }
        for (ssize_t index = 0; index < count; index++) {
            if (buffer[index] >= 'a' && buffer[index] <= 'z') {
                buffer[index] = (char)(buffer[index] - 'a' + 'A');
            }
        }
        write(1, buffer, (size_t)count);
        total += (int)count;
    }
    return total;
}
