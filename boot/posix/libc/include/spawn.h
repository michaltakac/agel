/* agel-libc: processes that make processes. There is no fork: a child is
   started by name from the program region with exactly the descriptors
   named here as its 0 and 1, the console as its 2, and its parent's
   namespace; nothing else is inherited. */
#ifndef AGEL_SPAWN_H
#define AGEL_SPAWN_H
#include <unistd.h>

#define AGEL_SPAWN_READ_ONLY 1

/* argv is NULL, giving the child its name as its one argument, or a
   NULL-terminated array that becomes the child's argv as it is. */
pid_t agel_spawn(const char *program, const char *const argv[], int stdin_fd, int stdout_fd, int flags);

#endif
