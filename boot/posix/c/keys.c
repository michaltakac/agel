/* Keys, down and up: a window that reports every key event it receives
   on the console, the way a game would hold and release them; the
   character q ends it. */
#include <agel/window.h>
#include <errno.h>
#include <stdio.h>

int main(void) {
    int window = agel_window(320, 120, "Keys");
    if (window < 0) {
        printf("keys: no display (errno %d)\n", errno);
        return 3;
    }
    agel_record label = agel_label(16, 16, AGEL_FACE_SANS, 14, 0x9e9e9e, "press keys; q ends it");
    agel_draw(window, &label, 1, AGEL_DRAW_CLEAR);
    unsigned downs = 0;
    for (;;) {
        agel_window_event event;
        if (agel_event(window, &event, 1) < 0) {
            return 4;
        }
        if (event.kind == AGEL_EVENT_KEY_DOWN) {
            downs++;
            printf("keys: down %d%s\n", event.key & 0xff,
                   (event.key & AGEL_KEY_EXTENDED) ? " extended" : "");
        } else if (event.kind == AGEL_EVENT_KEY_UP) {
            printf("keys: up %d%s\n", event.key & 0xff,
                   (event.key & AGEL_KEY_EXTENDED) ? " extended" : "");
        } else if (event.kind == AGEL_EVENT_KEY) {
            printf("keys: key %c\n", event.key);
            if (event.key == 'q') {
                printf("keys: quit after %u downs\n", downs);
                return 0;
            }
        }
    }
}
