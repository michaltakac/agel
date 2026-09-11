/* agel-libc: assert, reporting the failed expression and aborting. */
#ifndef AGEL_ASSERT_H
#define AGEL_ASSERT_H
#include <stdio.h>
#include <stdlib.h>

#ifdef NDEBUG
#define assert(expression) ((void)0)
#else
#define assert(expression) \
    ((expression) ? (void)0 : (fprintf(stderr, "%s:%d: assertion failed: %s\n", __FILE__, __LINE__, #expression), abort()))
#endif

#endif
