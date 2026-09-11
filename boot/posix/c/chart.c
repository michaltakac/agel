/* A window from C: a bar for each number on the command line, drawn
   through the desktop's window protocol. Without a display it says so and
   exits with 3; with the word "outside" among its arguments it first asks
   to draw past the window's edge, to show the request refused. */
#include <agel/window.h>
#include <errno.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#define WIDTH 480
#define HEIGHT 280
#define BARS 8

int main(int argc, char **argv) {
    int values[BARS];
    int count = 0;
    int outside = 0;
    for (int index = 1; index < argc; index++) {
        if (strcmp(argv[index], "outside") == 0) {
            outside = 1;
        } else if (count < BARS) {
            values[count++] = atoi(argv[index]);
        }
    }
    if (count == 0) {
        static const int series[] = {3, 7, 5, 9, 6};
        memcpy(values, series, sizeof series);
        count = 5;
    }
    int window = agel_window(WIDTH, HEIGHT, "Chart");
    if (window < 0) {
        printf("chart: no display (errno %d)\n", errno);
        return 3;
    }
    if (outside) {
        agel_record beyond = agel_rect(WIDTH - 10, 10, 40, 40, 0, 0xff0000);
        if (agel_draw(window, &beyond, 1, 0) < 0) {
            printf("chart: a rectangle outside the window was refused (errno %d)\n", errno);
        } else {
            printf("chart: a rectangle outside the window was accepted\n");
        }
    }
    int largest = 1;
    for (int index = 0; index < count; index++) {
        if (values[index] > largest) {
            largest = values[index];
        }
    }
    agel_record records[AGEL_DRAW_RECORDS];
    unsigned pending = 0;
    records[pending++] = agel_label(24, 16, AGEL_FACE_SANS_MEDIUM, 16, 0xdedede,
                                    "Numbers from the arguments");
    if (agel_draw(window, records, pending, AGEL_DRAW_CLEAR) < 0) {
        printf("chart: the title was refused (errno %d)\n", errno);
        return 4;
    }
    pending = 0;
    unsigned slot = (WIDTH - 48) / (unsigned)count;
    unsigned bar = slot * 3 / 4;
    unsigned room = HEIGHT - 96;
    for (int index = 0; index < count; index++) {
        int value = values[index] < 0 ? 0 : values[index];
        unsigned height = room * (unsigned)value / (unsigned)largest;
        if (height < 2) {
            height = 2;
        }
        unsigned x = 24 + (unsigned)index * slot + (slot - bar) / 2;
        unsigned y = HEIGHT - 48 - height;
        char text[16];
        snprintf(text, sizeof text, "%d", values[index]);
        records[pending++] = agel_rect(x, y, bar, height, 4, 0x63d0df);
        records[pending++] = agel_label(x, HEIGHT - 40, AGEL_FACE_MONO, 14, 0x9e9e9e, text);
        if (pending + 2 > AGEL_DRAW_RECORDS) {
            if (agel_draw(window, records, pending, 0) < 0) {
                printf("chart: bars refused (errno %d)\n", errno);
                return 4;
            }
            pending = 0;
        }
    }
    if (pending > 0 && agel_draw(window, records, pending, 0) < 0) {
        printf("chart: bars refused (errno %d)\n", errno);
        return 4;
    }
    printf("chart: window %d shows %d bars\n", window, count);
    return 0;
}
