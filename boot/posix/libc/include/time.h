/* agel-libc: one clock, monotonic, counting from the machine's bring-up
   in microseconds; there is no calendar, so time() counts from the boot. */
#ifndef AGEL_TIME_H
#define AGEL_TIME_H
#include <sys/types.h>

typedef long time_t;
typedef long clock_t;
#define CLOCKS_PER_SEC 1000000L

struct timespec {
    time_t tv_sec;
    long tv_nsec;
};

#define CLOCK_REALTIME 0
#define CLOCK_MONOTONIC 1
typedef int clockid_t;

clock_t clock(void);
time_t time(time_t *out);
int clock_gettime(clockid_t clock, struct timespec *out);
int nanosleep(const struct timespec *request, struct timespec *remaining);

#endif
