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
#include "doom/p_local.h"
#include "doom/r_main.h"
#include "doom/r_state.h"
#include "doom/tables.h"

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

/* The engine's map, walked. The level is laid over a grid of cells
   (32 map units, twice the player's radius); two neighbouring cells are
   joined when a walk between their centres crosses no line a player
   cannot cross (a one-sided wall, a blocking line, a ledge more than a
   step up, a gap shorter than a player; a closed door is crossed, since
   a door opens), tested with the engine's own line traversal and kept
   once tested, so the map is learned as it is walked. A breadth-first
   search from the player's cell to the cell in front of the level's
   exit line (a switch or a walk-over, normal or secret) is the route;
   the waypoint reported is the furthest cell along it that a straight
   walk from the player reaches, as a heading in the same 256ths of a
   turn as the player's angle and a distance in map units, with the
   route's remaining length and how much of the level the player has
   seen: the share of its lines the automap would draw. Three rays,
   ahead and a quarter turn to either side, report how far the player
   could walk before a line that cannot be crossed, at most 512 units,
   and whether what stops the ray ahead is a closed door. This is the
   engine's own data, read; nothing is decided here. */
#define RAY_UNITS 512
#define CELL 32
#define CELL_SHIFT 5
#define GRID_MAX 256
#define GRID_CELLS (GRID_MAX * GRID_MAX)
#define LOOKAHEAD 48

static int crossable(const line_t *line, const sector_t *from, const sector_t *to) {
    if (to == NULL || (line->flags & ML_BLOCKING)) {
        return 0;
    }
    /* A floor that hurts (nukage, blood, the damaging specials) is not
       walked into: the route goes round it, as a player would. */
    if (to->special == 4 || to->special == 5 || to->special == 7 || to->special == 16) {
        return 0;
    }
    if (to->floorheight - from->floorheight > 24 * FRACUNIT) {
        return 0;
    }
    int door = from->ceilingheight <= from->floorheight || to->ceilingheight <= to->floorheight;
    fixed_t ceiling = to->ceilingheight < from->ceilingheight ? to->ceilingheight : from->ceilingheight;
    fixed_t floor = to->floorheight > from->floorheight ? to->floorheight : from->floorheight;
    return door || ceiling - floor >= 56 * FRACUNIT;
}

static int ray_units, ray_door, ray_doors_stop;
static fixed_t ray_x, ray_y;

static boolean ray_stop(intercept_t *in) {
    if (!in->isaline) {
        return true;
    }
    line_t *line = in->d.line;
    int side = P_PointOnLineSide(ray_x, ray_y, line);
    const sector_t *from = side ? line->backsector : line->frontsector;
    const sector_t *to = side ? line->frontsector : line->backsector;
    if (from == NULL) {
        from = line->frontsector;
    }
    if (ray_doors_stop && to != NULL && to->ceilingheight <= to->floorheight && !(line->flags & ML_BLOCKING)) {
        ray_door = 1;
        ray_units = (int)(FixedMul(in->frac, RAY_UNITS * FRACUNIT) >> FRACBITS);
        return false;
    }
    if (crossable(line, from, to)) {
        return true;
    }
    ray_units = (int)(FixedMul(in->frac, RAY_UNITS * FRACUNIT) >> FRACBITS);
    return false;
}

static int ray(const player_t *player, angle_t angle, int *door) {
    ray_x = player->mo->x;
    ray_y = player->mo->y;
    ray_units = RAY_UNITS;
    ray_door = 0;
    ray_doors_stop = 1;
    fixed_t x2 = ray_x + FixedMul(finecosine[angle >> ANGLETOFINESHIFT], RAY_UNITS * FRACUNIT);
    fixed_t y2 = ray_y + FixedMul(finesine[angle >> ANGLETOFINESHIFT], RAY_UNITS * FRACUNIT);
    P_PathTraverse(ray_x, ray_y, x2, y2, PT_ADDLINES, ray_stop);
    if (door != NULL) {
        *door = ray_door;
    }
    return ray_units;
}

/* A straight walk from one point to another crosses nothing a player
   cannot cross. */
static int walkable(fixed_t x1, fixed_t y1, fixed_t x2, fixed_t y2) {
    ray_x = x1;
    ray_y = y1;
    ray_units = RAY_UNITS;
    ray_door = 0;
    ray_doors_stop = 0;
    return P_PathTraverse(x1, y1, x2, y2, PT_ADDLINES, ray_stop) ? 1 : 0;
}

static int grid_w, grid_h, grid_for_lines = -1;
static fixed_t grid_x0, grid_y0;
static unsigned char grid_known[GRID_CELLS], grid_open[GRID_CELLS];
static int grid_prev[GRID_CELLS], grid_queue[GRID_CELLS];
static const int grid_dx[8] = {1, -1, 0, 0, 1, 1, -1, -1};
static const int grid_dy[8] = {0, 0, 1, -1, 1, -1, 1, -1};

static void grid_for_level(void) {
    if (grid_for_lines == numlines) {
        return;
    }
    grid_for_lines = numlines;
    grid_x0 = bmaporgx;
    grid_y0 = bmaporgy;
    grid_w = bmapwidth * (MAPBLOCKUNITS / CELL);
    grid_h = bmapheight * (MAPBLOCKUNITS / CELL);
    if (grid_w > GRID_MAX) {
        grid_w = GRID_MAX;
    }
    if (grid_h > GRID_MAX) {
        grid_h = GRID_MAX;
    }
    memset(grid_known, 0, sizeof grid_known);
    memset(grid_open, 0, sizeof grid_open);
}

static fixed_t cell_x(int cx) { return grid_x0 + ((fixed_t)cx << (CELL_SHIFT + FRACBITS)) + (CELL / 2) * FRACUNIT; }
static fixed_t cell_y(int cy) { return grid_y0 + ((fixed_t)cy << (CELL_SHIFT + FRACBITS)) + (CELL / 2) * FRACUNIT; }

static int cell_open(int cx, int cy, int direction) {
    int cell = cy * grid_w + cx;
    unsigned char bit = (unsigned char)(1u << direction);
    if (!(grid_known[cell] & bit)) {
        int nx = cx + grid_dx[direction], ny = cy + grid_dy[direction];
        int open = nx >= 0 && ny >= 0 && nx < grid_w && ny < grid_h &&
                   walkable(cell_x(cx), cell_y(cy), cell_x(nx), cell_y(ny));
        grid_known[cell] |= bit;
        if (open) {
            grid_open[cell] |= bit;
        }
    }
    return (grid_open[cell] & bit) != 0;
}

static void exit_report(const player_t *player, unsigned *heading, int *distance, int *remaining, int *seen) {
    const line_t *exit_line = NULL;
    int mapped = 0;
    for (int i = 0; i < numlines; i++) {
        const line_t *line = &lines[i];
        if (line->flags & ML_MAPPED) {
            mapped++;
        }
        if (exit_line == NULL && (line->special == 11 || line->special == 51 || line->special == 52 || line->special == 124)) {
            exit_line = line;
        }
    }
    *seen = numlines > 0 ? mapped * 100 / numlines : 0;
    *heading = 0;
    *distance = -1;
    *remaining = -1;
    if (exit_line == NULL || player->mo == NULL) {
        return;
    }
    grid_for_level();
    /* The cell in front of the exit line: a step in from its midpoint on
       the side a player stands on to use it (the front). */
    fixed_t mid_x = exit_line->v1->x / 2 + exit_line->v2->x / 2;
    fixed_t mid_y = exit_line->v1->y / 2 + exit_line->v2->y / 2;
    fixed_t length = P_AproxDistance(exit_line->dx, exit_line->dy);
    if (length == 0) {
        return;
    }
    fixed_t normal_x = FixedDiv(exit_line->dy, length), normal_y = -FixedDiv(exit_line->dx, length);
    fixed_t target_x = mid_x + normal_x * 24, target_y = mid_y + normal_y * 24;
    int sx = (int)((player->mo->x - grid_x0) >> (CELL_SHIFT + FRACBITS));
    int sy = (int)((player->mo->y - grid_y0) >> (CELL_SHIFT + FRACBITS));
    int tx = (int)((target_x - grid_x0) >> (CELL_SHIFT + FRACBITS));
    int ty = (int)((target_y - grid_y0) >> (CELL_SHIFT + FRACBITS));
    if (sx < 0 || sy < 0 || sx >= grid_w || sy >= grid_h || tx < 0 || ty < 0 || tx >= grid_w || ty >= grid_h) {
        return;
    }
    int start = sy * grid_w + sx, goal = ty * grid_w + tx;
    int cells = grid_w * grid_h;
    for (int i = 0; i < cells; i++) {
        grid_prev[i] = -1;
    }
    grid_prev[start] = start;
    int head = 0, tail = 0;
    grid_queue[tail++] = start;
    while (head < tail && grid_prev[goal] == -1) {
        int here = grid_queue[head++];
        int cx = here % grid_w, cy = here / grid_w;
        for (int direction = 0; direction < 8; direction++) {
            if (!cell_open(cx, cy, direction)) {
                continue;
            }
            int next = (cy + grid_dy[direction]) * grid_w + cx + grid_dx[direction];
            if (grid_prev[next] == -1) {
                grid_prev[next] = here;
                grid_queue[tail++] = next;
            }
        }
    }
    if (grid_prev[goal] == -1) {
        return;
    }
    /* The route, start first, reusing the queue as scratch. */
    int steps = 0;
    for (int at = goal; at != start; at = grid_prev[at]) {
        steps++;
    }
    int at = goal;
    for (int i = steps - 1; i >= 0; i--) {
        grid_queue[i] = at;
        at = grid_prev[at];
    }
    *remaining = steps * CELL;
    if (steps == 0) {
        *heading = (unsigned)(R_PointToAngle2(player->mo->x, player->mo->y, mid_x, mid_y) >> 24);
        *distance = P_AproxDistance(mid_x - player->mo->x, mid_y - player->mo->y) >> FRACBITS;
        return;
    }
    /* Pull the waypoint along the route while a straight walk reaches it. */
    int waypoint = grid_queue[0];
    for (int i = 1; i < steps && i < LOOKAHEAD; i++) {
        int cell = grid_queue[i];
        if (!walkable(player->mo->x, player->mo->y, cell_x(cell % grid_w), cell_y(cell / grid_w))) {
            break;
        }
        waypoint = cell;
    }
    fixed_t goal_x = cell_x(waypoint % grid_w), goal_y = cell_y(waypoint / grid_w);
    *heading = (unsigned)(R_PointToAngle2(player->mo->x, player->mo->y, goal_x, goal_y) >> 24);
    *distance = P_AproxDistance(goal_x - player->mo->x, goal_y - player->mo->y) >> FRACBITS;
}

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
            unsigned heading;
            int distance, remaining, seen, door;
            exit_report(player, &heading, &distance, &remaining, &seen);
            int ahead = ray(player, player->mo->angle, &door);
            int left = ray(player, player->mo->angle + ANG45, NULL);
            int right = ray(player, player->mo->angle - ANG45, NULL);
            printf("doom: state map %d x %d y %d angle %u health %d armor %d ammo %d kills %d goal %u dist %d path %d seen %d free %d %d %d door %d%s\n",
                   gamemap, player->mo->x >> FRACBITS, player->mo->y >> FRACBITS,
                   (unsigned)(player->mo->angle >> 24), player->health, player->armorpoints,
                   player->ammo[player->readyweapon == wp_pistol || player->readyweapon == wp_chaingun ? am_clip : am_shell],
                   player->killcount, heading, distance, remaining, seen, ahead, left, right, door, paused ? " paused" : "");
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
