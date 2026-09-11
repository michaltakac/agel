/* A window that listens: a press in its content puts a dot where it
   landed and the pointer moves the dot until the button is released, which
   fixes it; each press and release is a line on the console. The key q
   ends the program, any other key clears the dots. The process sleeps in
   agel_event between events, so the desktop runs meanwhile. */
#include <agel/window.h>
#include <errno.h>
#include <stdio.h>

#define DOTS 20
#define RADIUS 12

/* The content's size, as opened and as the operator resizes it. */
static unsigned width = 400;
static unsigned height = 300;

static agel_record dot_at(int x, int y) {
    unsigned cx = x < RADIUS ? RADIUS : (unsigned)x;
    unsigned cy = y < RADIUS ? RADIUS : (unsigned)y;
    if (cx + RADIUS > width) cx = width - RADIUS;
    if (cy + RADIUS > height) cy = height - RADIUS;
    return agel_ellipse(cx, cy, RADIUS, RADIUS, 0xe79cfe);
}

int main(void) {
    int window = agel_window(width, height, "Sketch");
    if (window < 0) {
        printf("sketch: no display (errno %d)\n", errno);
        return 3;
    }
    agel_record records[DOTS + 2];
    records[0] = agel_label(16, 16, AGEL_FACE_SANS, 14, 0x9e9e9e, "press, drag; q ends it");
    unsigned fixed = 0;
    int holding = 0;
    agel_draw(window, records, 1, AGEL_DRAW_CLEAR);
    for (;;) {
        agel_window_event event;
        if (agel_event(window, &event, 1) < 0) {
            printf("sketch: no events (errno %d)\n", errno);
            return 4;
        }
        if (event.kind == AGEL_EVENT_KEY) {
            if (event.key == 'q') {
                printf("sketch: quit after %u dots\n", fixed);
                return 0;
            }
            fixed = 0;
            holding = 0;
            agel_draw(window, records, 1, AGEL_DRAW_CLEAR);
            printf("sketch: cleared\n");
            continue;
        }
        if (event.kind == AGEL_EVENT_RESIZE) {
            width = (unsigned)event.x;
            height = (unsigned)event.y;
            printf("sketch: resized to %ux%u\n", width, height);
            continue;
        }
        if (event.kind == AGEL_EVENT_PRESS) {
            if (fixed == DOTS) {
                fixed = 0;
            }
            holding = 1;
            printf("sketch: press at %d,%d\n", event.x, event.y);
        } else if (event.kind == AGEL_EVENT_MOTION) {
            if (!holding) {
                continue;
            }
        } else if (event.kind == AGEL_EVENT_RELEASE) {
            if (!holding) {
                continue;
            }
            holding = 0;
            fixed++;
            printf("sketch: release at %d,%d\n", event.x, event.y);
        } else {
            continue;
        }
        /* The fixed dots and the one the pointer holds: the whole window,
           again, so the moving dot leaves nothing behind. */
        records[1 + (holding ? fixed : fixed - 1)] = dot_at(event.x, event.y);
        if (agel_draw(window, records, 1 + fixed + holding, AGEL_DRAW_CLEAR) < 0) {
            printf("sketch: draw refused (errno %d)\n", errno);
            return 4;
        }
    }
}
