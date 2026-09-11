/* A window that listens: every press in its content becomes a dot where
   it landed and a line on the console; the key q ends the program, any
   other key clears the dots. The process sleeps in agel_event between
   presses, so the desktop runs meanwhile. */
#include <agel/window.h>
#include <errno.h>
#include <stdio.h>

#define WIDTH 400
#define HEIGHT 300

int main(void) {
    int window = agel_window(WIDTH, HEIGHT, "Sketch");
    if (window < 0) {
        printf("sketch: no display (errno %d)\n", errno);
        return 3;
    }
    agel_record hint = agel_label(16, 16, AGEL_FACE_SANS, 14, 0x9e9e9e, "click here; q ends it");
    agel_draw(window, &hint, 1, AGEL_DRAW_CLEAR);
    int dots = 0;
    for (;;) {
        agel_window_event event;
        int got = agel_event(window, &event, 1);
        if (got < 0) {
            printf("sketch: no events (errno %d)\n", errno);
            return 4;
        }
        if (event.kind == AGEL_EVENT_KEY) {
            if (event.key == 'q') {
                printf("sketch: quit after %d dots\n", dots);
                return 0;
            }
            agel_draw(window, &hint, 1, AGEL_DRAW_CLEAR);
            dots = 0;
            printf("sketch: cleared\n");
        } else if (event.kind == AGEL_EVENT_PRESS) {
            unsigned x = (unsigned)event.x;
            unsigned y = (unsigned)event.y;
            unsigned radius = 12;
            if (x < radius) x = radius;
            if (y < radius) y = radius;
            if (x + radius > WIDTH) x = WIDTH - radius;
            if (y + radius > HEIGHT) y = HEIGHT - radius;
            agel_record dot = agel_ellipse(x, y, radius, radius, 0xe79cfe);
            if (agel_draw(window, &dot, 1, 0) < 0) {
                /* The window is full: start over with this dot. */
                agel_record fresh[2] = {hint, dot};
                agel_draw(window, fresh, 2, AGEL_DRAW_CLEAR);
                dots = 0;
            }
            dots++;
            printf("sketch: press at %d,%d\n", event.x, event.y);
        }
    }
}
