/* agel-libc: the one signal there is, sent to one's own child. A process
   cannot catch a signal; a killed child simply ends, and its parent's
   waitpid sees the signal. */
#ifndef AGEL_SIGNAL_H
#define AGEL_SIGNAL_H
#include <sys/types.h>

#define SIGKILL 9
#define SIGSEGV 11
#define SIGTERM 15

int kill(pid_t child, int signal);

#endif
