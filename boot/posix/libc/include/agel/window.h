/* agel-libc: windows on the Agel desktop. Not POSIX: the desktop's window
   protocol, in which a process asks the supervisor for a window and hands
   it compositor records, relative to the window's content, that the
   supervisor checks and keeps. A record the window does not permit (an
   operation not listed here, or one reaching outside the content) fails
   the whole request with EINVAL and draws nothing. Where there is no
   display, agel_window fails with ENODEV. */
#ifndef AGEL_WINDOW_H
#define AGEL_WINDOW_H
#include <stddef.h>
#include <string.h>

/* One compositor record: 64 bytes, sixteen little-endian words. */
typedef struct {
    unsigned int word[16];
} agel_record;

#define AGEL_DRAW_CLEAR 1u
/* At most this many records a window holds, and per request. */
#define AGEL_WINDOW_RECORDS 24u
#define AGEL_DRAW_RECORDS 8u

/* The faces the desktop's atlases carry. */
#define AGEL_FACE_SANS 0u
#define AGEL_FACE_SANS_MEDIUM 1u
#define AGEL_FACE_MONO 2u

/* A window of width by height pixels of content (64x48 up to 1280x720)
   with a title of at most 28 bytes: its number from 0, or -1 with errno. */
int agel_window(unsigned width, unsigned height, const char *title);

/* Draw count records into the window, clearing it first with
   AGEL_DRAW_CLEAR in flags: the records the window now holds, or -1 with
   errno and nothing of this request drawn. */
int agel_draw(int window, const agel_record *records, unsigned count, unsigned flags);

/* What a window receives: a press in its content, at x and y relative to
   the content; a key while the window has the keyboard (it has it from
   when it opens or is clicked until the workshop is clicked); and, after
   a press, the pointer's motion (coalesced to the latest) and its release,
   with content coordinates, which may lie outside the content. */
#define AGEL_EVENT_PRESS 1
#define AGEL_EVENT_KEY 2
#define AGEL_EVENT_RELEASE 3
#define AGEL_EVENT_MOTION 4

typedef struct {
    int kind;
    int x;
    int y;
    int key;
} agel_window_event;

/* The next event for the window: 1 with it filled in, 0 when there is none
   and wait is zero, or -1 with errno. With wait nonzero the process sleeps
   until there is one, and the desktop runs meanwhile. */
int agel_event(int window, agel_window_event *event, int wait);

/* Record builders; colours are 0xRRGGBB, alphas 0 to 255. */
static inline agel_record agel_rect(unsigned x, unsigned y, unsigned width, unsigned height,
                                    unsigned radius, unsigned colour) {
    agel_record record = {{2, x, y, width, height, radius, colour}};
    return record;
}

static inline agel_record agel_gradient(unsigned x, unsigned y, unsigned width, unsigned height,
                                        unsigned radius, unsigned top, unsigned bottom) {
    agel_record record = {{3, x, y, width, height, radius, top, bottom}};
    return record;
}

static inline agel_record agel_ellipse(unsigned cx, unsigned cy, unsigned rx, unsigned ry,
                                       unsigned colour) {
    agel_record record = {{4, cx, cy, rx, ry, colour}};
    return record;
}

static inline agel_record agel_surface(unsigned x, unsigned y, unsigned width, unsigned height,
                                       unsigned radius, unsigned colour, unsigned alpha) {
    agel_record record = {{7, x, y, width, height, radius, colour, alpha}};
    return record;
}

static inline agel_record agel_sprite(unsigned x, unsigned y, unsigned index, unsigned tint) {
    agel_record record = {{9, x, y, index, tint, 255}};
    return record;
}

/* Text in a face at a size, at most 28 bytes of it. */
static inline agel_record agel_label(unsigned x, unsigned y, unsigned face, unsigned size,
                                     unsigned colour, const char *text) {
    agel_record record = {{6, x, y, face, size, colour, 255, 0, 0}};
    size_t length = strlen(text);
    if (length > 28) {
        length = 28;
    }
    record.word[8] = (unsigned)length;
    memcpy(&record.word[9], text, length);
    return record;
}

#endif
