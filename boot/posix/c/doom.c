/* DOOM on Agel: the doomgeneric engine (boot/posix/c/doom/, GPL v2, its
   licence beside it) bound to the desktop's window protocol. The engine
   renders 320 by 200 pixels into a buffer it owns; this file copies each
   frame into the window's canvas and blits it at twice its size, turns the
   window's key events into the engine's, and gives it the clock and sleep
   it asks for. Start it with the shareware data in the data region:
       :exec c-doom -- -iwad /data/doom1.wad -mb 8
   and -timedemo demo1 to have it report its frame rate and exit. */
#include <agel/window.h>
#include <errno.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>

#include "doom/doomgeneric.h"
#include "doom/doomkeys.h"
#include "doom/doomstat.h"

#define WIDTH 320u
#define HEIGHT 200u
#define QUEUE 64u

static int window = -1;
static unsigned *canvas;
static unsigned short queue[QUEUE];
static unsigned head;
static unsigned tail;

static void push(int pressed, unsigned char key) {
    if (tail - head == QUEUE) {
        head++;
    }
    queue[tail++ % QUEUE] = (unsigned short)((pressed << 8) | key);
}

/* A key of the desktop's, as the engine names it; zero for one it has no
   name for. This engine names its fire, use and strafe keys apart from
   the keys that carry them: control fires, space uses, comma and period
   strafe, alt with an arrow strafes too, shift runs, and p pauses. */
static unsigned char doom_key(int code) {
    unsigned scan = (unsigned)code & 0xffu;
    if (code & AGEL_KEY_EXTENDED) {
        switch (scan) {
        case 0x48: return KEY_UPARROW;
        case 0x50: return KEY_DOWNARROW;
        case 0x4b: return KEY_LEFTARROW;
        case 0x4d: return KEY_RIGHTARROW;
        case 0x1d: return KEY_FIRE;
        case 0x38: return KEY_RALT;
        case 0x1c: return KEY_ENTER;
        case 0x47: return KEY_HOME;
        case 0x4f: return KEY_END;
        case 0x49: return KEY_PGUP;
        case 0x51: return KEY_PGDN;
        case 0x52: return KEY_INS;
        default: return 0;
        }
    }
    /* p pauses: the engine's pause key is the Pause key, which the
       desktop's keyboard decoder has no code for. */
    if (scan == 0x19) return KEY_PAUSE;
    if (scan >= 0x10 && scan <= 0x18) return (unsigned char)"qwertyuio"[scan - 0x10];
    if (scan >= 0x1e && scan <= 0x26) return (unsigned char)"asdfghjkl"[scan - 0x1e];
    if (scan >= 0x2c && scan <= 0x32) return (unsigned char)"zxcvbnm"[scan - 0x2c];
    if (scan >= 0x02 && scan <= 0x0b) return (unsigned char)"1234567890"[scan - 0x02];
    if (scan >= 0x3b && scan <= 0x44) return (unsigned char)(KEY_F1 + (scan - 0x3b));
    switch (scan) {
    case 0x01: return KEY_ESCAPE;
    case 0x1c: return KEY_ENTER;
    case 0x0f: return KEY_TAB;
    case 0x0e: return KEY_BACKSPACE;
    case 0x39: return KEY_USE;
    case 0x1d: return KEY_FIRE;
    case 0x2a: case 0x36: return KEY_RSHIFT;
    case 0x38: return KEY_RALT;
    case 0x0c: return KEY_MINUS;
    case 0x0d: return KEY_EQUALS;
    case 0x33: return KEY_STRAFE_L;
    case 0x34: return KEY_STRAFE_R;
    case 0x3a: return KEY_CAPSLOCK;
    case 0x57: return KEY_F11;
    case 0x58: return KEY_F12;
    default: return 0;
    }
}

/* Everything the window has queued: keys down and up from the machine's
   keyboard, and a character from the serial console as a tap. */
static void drain(void) {
    agel_window_event event;
    while (agel_event(window, &event, 0) > 0) {
        if (event.kind == AGEL_EVENT_KEY_DOWN || event.kind == AGEL_EVENT_KEY_UP) {
            unsigned char key = doom_key(event.key);
            if (key) {
                push(event.kind == AGEL_EVENT_KEY_DOWN, key);
            }
        } else if (event.kind == AGEL_EVENT_KEY) {
            unsigned char key = event.key == '\n' ? KEY_ENTER : event.key == 27 ? KEY_ESCAPE : (unsigned char)event.key;
            push(1, key);
            push(0, key);
        }
    }
}

void DG_Init(void) {
    window = agel_window(WIDTH * 2, HEIGHT * 2, "DOOM");
    if (window < 0) {
        printf("doom: no display (errno %d)\n", errno);
        exit(3);
    }
    canvas = agel_canvas(window, WIDTH, HEIGHT);
    if (canvas == NULL) {
        printf("doom: no canvas (errno %d)\n", errno);
        exit(4);
    }
}

static unsigned frames;

void DG_DrawFrame(void) {
    memcpy(canvas, DG_ScreenBuffer, WIDTH * HEIGHT * 4);
    agel_record blit = agel_blit(0, 0, 2);
    if (agel_draw(window, &blit, 1, AGEL_DRAW_CLEAR) < 0) {
        printf("doom: blit refused (errno %d)\n", errno);
    }
    /* A heartbeat on the console every ten seconds of game time, so a run
       can be followed and timed from outside, and the engine's own account
       of the player each time the game is paused: one line for whoever
       plays from outside, and the dataset such a run leaves. The console
       is a panel under the window, repainted for every line, so lines are
       few. */
    static int was_paused;
    int heartbeat = frames++ % 350 == 0;
    if (heartbeat) {
        printf("doom: frame %u at %u ms\n", frames - 1, DG_GetTicksMs());
    }
    if (heartbeat || (paused && !was_paused)) {
        player_t *player = &players[consoleplayer];
        if (player->mo != NULL) {
            printf("doom: state map %d x %d y %d angle %u health %d armor %d ammo %d kills %d%s\n",
                   gamemap, player->mo->x >> FRACBITS, player->mo->y >> FRACBITS,
                   (unsigned)(player->mo->angle >> 24), player->health, player->armorpoints,
                   player->ammo[player->readyweapon == wp_pistol || player->readyweapon == wp_chaingun ? am_clip : am_shell],
                   player->killcount, paused ? " paused" : "");
        }
    }
    was_paused = paused;
    drain();
}

void DG_SleepMs(uint32_t ms) {
    static int reported;
    if (!reported) {
        reported = 1;
        printf("doom: first sleep of %u ms at %u ms\n", ms, DG_GetTicksMs());
    }
    struct timespec pause = {(time_t)(ms / 1000u), (long)(ms % 1000u) * 1000000L};
    nanosleep(&pause, NULL);
    drain();
}

uint32_t DG_GetTicksMs(void) {
    struct timespec now;
    clock_gettime(CLOCK_MONOTONIC, &now);
    return (uint32_t)((unsigned long)now.tv_sec * 1000ul + (unsigned long)now.tv_nsec / 1000000ul);
}

int DG_GetKey(int *pressed, unsigned char *key) {
    drain();
    if (head == tail) {
        return 0;
    }
    unsigned short packed = queue[head++ % QUEUE];
    *pressed = packed >> 8;
    *key = (unsigned char)(packed & 0xff);
    return 1;
}

void DG_SetWindowTitle(const char *title) { (void)title; }

int main(int argc, char **argv) {
    doomgeneric_Create(argc, argv);
    for (;;) {
        doomgeneric_Tick();
    }
}
