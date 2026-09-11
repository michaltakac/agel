/* Time and signals from C: the monotonic clock advances across a sleep,
   longjmp returns through setjmp, the environment holds what is put in
   it, and a sleeping child is killed and seen by its parent as signalled. */
#include <errno.h>
#include <setjmp.h>
#include <signal.h>
#include <spawn.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/wait.h>
#include <time.h>
#include <unistd.h>

static jmp_buf escape;

static void deep(int depth) {
    if (depth == 0) {
        longjmp(escape, 7);
    }
    deep(depth - 1);
}

int main(void) {
    struct timespec before;
    struct timespec after;
    clock_gettime(CLOCK_MONOTONIC, &before);
    struct timespec nap = {0, 20 * 1000 * 1000};
    if (nanosleep(&nap, NULL) != 0) {
        printf("clock: nanosleep: errno %d\n", errno);
        return 1;
    }
    clock_gettime(CLOCK_MONOTONIC, &after);
    long elapsed = (after.tv_sec - before.tv_sec) * 1000000L + (after.tv_nsec - before.tv_nsec) / 1000;
    if (elapsed < 20000) {
        printf("clock: FAILED: slept %ld microseconds\n", elapsed);
        return 1;
    }
    printf("clock: slept, the monotonic clock advanced at least 20 ms\n");
    if (time(NULL) < 0 || clock() < 0) {
        printf("clock: FAILED: time or clock negative\n");
        return 1;
    }

    int returned = setjmp(escape);
    if (returned == 0) {
        deep(3);
        printf("clock: FAILED: longjmp did not return\n");
        return 1;
    }
    printf("clock: longjmp returned %d\n", returned);

    if (getenv("HOME") != NULL) {
        printf("clock: FAILED: HOME set at the start\n");
        return 1;
    }
    setenv("HOME", "/app", 1);
    setenv("HOME", "/elsewhere", 0);
    printf("clock: HOME=%s\n", getenv("HOME"));
    unsetenv("HOME");
    printf("clock: HOME %s\n", getenv("HOME") == NULL ? "unset" : "still set");

    pid_t child = agel_spawn("c-nap", NULL, -1, 1, 0);
    if (child < 0) {
        printf("clock: spawn nap: errno %d\n", errno);
        return 1;
    }
    if (kill(child, SIGTERM) == 0 || errno != EINVAL) {
        printf("clock: FAILED: SIGTERM accepted\n");
        return 1;
    }
    if (kill(child, SIGKILL) != 0) {
        printf("clock: kill: errno %d\n", errno);
        return 1;
    }
    int status = 0;
    if (waitpid(child, &status, 0) != child) {
        printf("clock: waitpid: errno %d\n", errno);
        return 1;
    }
    if (WIFSIGNALED(status)) {
        printf("clock: nap killed by signal %d\n", WTERMSIG(status));
    } else {
        printf("clock: FAILED: nap exited with %d\n", WEXITSTATUS(status));
        return 1;
    }
    printf("clock: kill again: errno %d\n", kill(child, SIGKILL) == 0 ? 0 : errno);
    return 0;
}
