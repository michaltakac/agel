/* A canvas: pixels a process draws into pages of its own, shown in its
   window at twice their size. Each frame is a gradient (red across, green
   down) with a white bar that moves; the frame's time is printed. With a
   count on the command line that many frames are drawn and the program
   ends; with none it draws on each event until q. */
#include <agel/window.h>
#include <errno.h>
#include <stdio.h>
#include <stdlib.h>
#include <time.h>

#define WIDTH 320u
#define HEIGHT 200u

static unsigned long microseconds(void) {
    struct timespec now;
    clock_gettime(CLOCK_MONOTONIC, &now);
    return (unsigned long)now.tv_sec * 1000000ul + (unsigned long)now.tv_nsec / 1000ul;
}

static void paint(unsigned *pixels, unsigned frame) {
    unsigned bar = (frame * 40u) % WIDTH;
    for (unsigned y = 0; y < HEIGHT; y++) {
        unsigned green = y * 255u / (HEIGHT - 1);
        for (unsigned x = 0; x < WIDTH; x++) {
            unsigned red = x * 255u / (WIDTH - 1);
            pixels[y * WIDTH + x] = (x >= bar && x < bar + 8u)
                                        ? 0xffffffu
                                        : (red << 16) | (green << 8) | 0x40u;
        }
    }
}

int main(int argc, char **argv) {
    unsigned frames = argc > 1 ? (unsigned)atoi(argv[1]) : 0;
    int window = agel_window(WIDTH * 2, HEIGHT * 2, "Canvas");
    if (window < 0) {
        printf("canvas: no display (errno %d)\n", errno);
        return 3;
    }
    unsigned *pixels = agel_canvas(window, WIDTH, HEIGHT);
    if (pixels == NULL) {
        printf("canvas: refused (errno %d)\n", errno);
        return 4;
    }
    agel_record blit = agel_blit(0, 0, 2);
    unsigned frame = 0;
    for (;;) {
        unsigned long started = microseconds();
        paint(pixels, frame);
        if (agel_draw(window, &blit, 1, AGEL_DRAW_CLEAR) < 0) {
            printf("canvas: blit refused (errno %d)\n", errno);
            return 5;
        }
        printf("canvas: frame %u in %lu us\n", frame, microseconds() - started);
        frame++;
        if (frames != 0) {
            if (frame == frames) {
                break;
            }
            continue;
        }
        agel_window_event event;
        if (agel_event(window, &event, 1) < 0) {
            return 6;
        }
        if (event.kind == AGEL_EVENT_KEY && event.key == 'q') {
            break;
        }
    }
    printf("canvas: %u frames\n", frame);
    return 0;
}
