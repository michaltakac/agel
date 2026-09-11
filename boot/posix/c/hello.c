/* The first C program built for Agel from source: it exercises the console,
   the heap, the string routines and printf, and exits with a status. */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

int main(void) {
    char *text = malloc(32);
    if (text == NULL) {
        puts("hello: malloc failed");
        return 1;
    }
    strcpy(text, "a heap string");
    printf("hello from C on Agel: %s of %zu bytes, %d%% sure, %x hex\n", text, strlen(text), 100, 255);
    free(text);
    return 7;
}
