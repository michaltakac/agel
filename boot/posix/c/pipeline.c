/* A parent: makes a pipe, spawns a child with the pipe's read end as its
   standard input and the console as its output, feeds it, waits, and
   reports. Then spawns a program that faults and a name that does not
   exist, to show what a parent sees of each. */
#include <errno.h>
#include <spawn.h>
#include <stdio.h>
#include <string.h>
#include <sys/wait.h>
#include <unistd.h>

int main(void) {
    int ends[2];
    if (pipe(ends) != 0) {
        printf("pipeline: pipe: errno %d\n", errno);
        return 1;
    }
    pid_t child = agel_spawn("c-shout", ends[0], 1, 0);
    if (child < 0) {
        printf("pipeline: spawn: errno %d\n", errno);
        return 1;
    }
    /* The parent's read end is closed so that the child's is the last one:
       when the parent closes the write end, the child sees the end. */
    close(ends[0]);
    const char *text = "hello from the parent\n";
    write(ends[1], text, strlen(text));
    close(ends[1]);
    int status = 0;
    if (waitpid(child, &status, 0) != child) {
        printf("pipeline: waitpid: errno %d\n", errno);
        return 1;
    }
    printf("child %d exited with %d\n", child, WEXITSTATUS(status));

    pid_t hostile = agel_spawn("hostile", -1, 1, 0);
    if (hostile < 0) {
        printf("pipeline: spawn hostile: errno %d\n", errno);
        return 1;
    }
    waitpid(hostile, &status, 0);
    if (WIFSIGNALED(status)) {
        printf("child %d stopped by signal %d\n", hostile, WTERMSIG(status));
    } else {
        printf("child %d exited with %d\n", hostile, WEXITSTATUS(status));
    }

    if (agel_spawn("nothing", -1, 1, 0) < 0) {
        printf("spawn nothing: errno %d\n", errno);
    }
    if (waitpid(7, &status, 0) < 0) {
        printf("wait for no child: errno %d\n", errno);
    }
    return 0;
}
