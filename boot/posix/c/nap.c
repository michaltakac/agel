/* A child that sleeps for a long time; its parent ends it. */
#include <stdio.h>
#include <unistd.h>

int main(void) {
    sleep(30);
    printf("nap: woke, which should not happen\n");
    return 0;
}
