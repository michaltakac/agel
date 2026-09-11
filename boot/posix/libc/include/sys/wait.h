/* agel-libc: waiting for a child made with agel_spawn. */
#ifndef AGEL_SYS_WAIT_H
#define AGEL_SYS_WAIT_H
#include <unistd.h>

pid_t waitpid(pid_t child, int *status, int options);

#define WIFEXITED(status) (((status) & 0x7f) == 0)
#define WEXITSTATUS(status) (((status) >> 8) & 0xff)
#define WIFSIGNALED(status) (((status) & 0x7f) != 0)
#define WTERMSIG(status) ((status) & 0x7f)

#endif
