/* padui - WalPad 8G on-device UI: touch + buttons + display + status.
 * Freestanding aarch64 Linux, raw syscalls, no libc.  Runs from the boot ramdisk.
 *
 * Panel facts (from scripts/terminal_ui.py / display_dashboard.py):
 *   fb0 800x1280, 32bpp, memory order B,G,R,A, three pages (PAGE = 800*1280*4),
 *   present with sysfs pan "0,1280" then "0,0"; wake the panel once with blank 1->0.
 *   touch = /dev/input/event4 (ABS_MT_POSITION_X/Y, native 800x1280)
 *   keys  = event1 (mtk-kpd, volume) and event2 (mtk-pmic-keys, power)
 */
typedef unsigned long u64; typedef long i64; typedef unsigned int u32; typedef int i32;
typedef unsigned short u16; typedef unsigned char u8;

#include "font.h"

#define W 800
#define H 1280
#define PAGE_PX (W * H)
#define PAGE_BYTES (PAGE_PX * 4)

/* ---------------------------------------------------------------- syscalls */
static long sc6(long n, long a, long b, long c, long d, long e, long f) {
    register long x8 __asm__("x8") = n;
    register long x0 __asm__("x0") = a;
    register long x1 __asm__("x1") = b;
    register long x2 __asm__("x2") = c;
    register long x3 __asm__("x3") = d;
    register long x4 __asm__("x4") = e;
    register long x5 __asm__("x5") = f;
    __asm__ volatile("svc 0" : "+r"(x0) : "r"(x8), "r"(x1), "r"(x2), "r"(x3), "r"(x4), "r"(x5) : "memory", "cc");
    return x0;
}
#define sc3(n, a, b, c) sc6(n, (long)(a), (long)(b), (long)(c), 0, 0, 0)
#define SYS_openat 56
#define SYS_close 57
#define SYS_read 63
#define SYS_write 64
#define SYS_ppoll 73
#define SYS_nanosleep 101
#define SYS_mmap 222
#define SYS_clock_gettime 113
#define SYS_exit 93
#define AT_FDCWD (-100)
#define O_RDONLY 0
#define O_WRONLY 1
#define O_RDWR 2
#define O_NONBLOCK 04000

static long sys_open(const char *p, int fl) { return sc3(SYS_openat, AT_FDCWD, p, fl); }
static void sys_close(long fd) { sc3(SYS_close, fd, 0, 0); }

/* compiler-visible helpers (no libc); loops are barriered so they are not turned into calls to themselves */
void *memcpy(void *d, const void *s, unsigned long n) { u8 *a = d; const u8 *b = s; while (n--) { *a++ = *b++; __asm__ volatile(""); } return d; }
void *memset(void *d, int c, unsigned long n) { u8 *a = d; while (n--) { *a++ = (u8)c; __asm__ volatile(""); } return d; }

static int slen(const char *s) { int n = 0; while (s[n]) n++; return n; }

static void write_file(const char *path, const char *s) {
    long fd = sys_open(path, O_WRONLY);
    if (fd < 0) return;
    sc3(SYS_write, fd, s, slen(s));
    sys_close(fd);
}
static int read_file(const char *path, char *buf, int n) {
    long fd = sys_open(path, O_RDONLY);
    if (fd < 0) { buf[0] = 0; return -1; }
    long r = sc3(SYS_read, fd, buf, n - 1);
    sys_close(fd);
    if (r < 0) r = 0;
    buf[r] = 0;
    while (r > 0 && (buf[r - 1] == '\n' || buf[r - 1] == ' ')) buf[--r] = 0;
    return (int)r;
}
static void msleep(long ms) {
    struct { long s; long ns; } ts = { ms / 1000, (ms % 1000) * 1000000L };
    sc3(SYS_nanosleep, &ts, 0, 0);
}
static void itoa_(long v, char *o) {
    char t[24]; int i = 0, neg = v < 0; unsigned long u = neg ? -v : v;
    if (!u) t[i++] = '0';
    while (u) { t[i++] = '0' + u % 10; u /= 10; }
    int j = 0; if (neg) o[j++] = '-';
    while (i) o[j++] = t[--i];
    o[j] = 0;
}
static long atoi_(const char *s) { long v = 0, neg = 0; if (*s == '-') { neg = 1; s++; } while (*s >= '0' && *s <= '9') v = v * 10 + (*s++ - '0'); return neg ? -v : v; }
static void scat(char *d, const char *s) { while (*d) d++; while ((*d++ = *s++)); }
static void scpy(char *d, const char *s) { while ((*d++ = *s++)); }

/* ---------------------------------------------------------------- graphics */
static u32 *fbmem;            /* 3 pages */
static u32 back[PAGE_PX];     /* RAM copy of the current frame */
static int display_inited;

#define RGB(r, g, b) (0xFF000000u | ((u32)(r) << 16) | ((u32)(g) << 8) | (u32)(b))
#define C_BG RGB(11, 14, 22)
#define C_PANEL RGB(24, 30, 46)
#define C_TILE RGB(34, 52, 90)
#define C_TILE2 RGB(40, 84, 66)
#define C_TILE3 RGB(98, 62, 40)
#define C_TILE4 RGB(76, 48, 98)
#define C_FG RGB(232, 236, 244)
#define C_DIM RGB(140, 150, 170)
#define C_ACC RGB(80, 170, 255)
#define C_OK RGB(70, 210, 120)
#define C_WARN RGB(255, 190, 60)
#define C_BAD RGB(255, 90, 90)

static void px(int x, int y, u32 c) {
    if ((unsigned)x >= W || (unsigned)y >= H) return;
    int i = y * W + x;
    back[i] = c;
    if (fbmem) { fbmem[i] = c; fbmem[PAGE_PX + i] = c; fbmem[2 * PAGE_PX + i] = c; }
}
static void rect_back(int x, int y, int w, int h, u32 c) {   /* to RAM only */
    for (int j = y; j < y + h; j++) {
        if ((unsigned)j >= H) continue;
        for (int i = x; i < x + w; i++) if ((unsigned)i < W) back[j * W + i] = c;
    }
}
static void blit_all(void) {
    if (!fbmem) return;
    for (int p = 0; p < 3; p++) memcpy(fbmem + p * PAGE_PX, back, PAGE_BYTES);
}
static void commit(void) {
    write_file("/sys/class/graphics/fb0/pan", "0,1280");
    write_file("/sys/class/graphics/fb0/pan", "0,0");
    if (!display_inited) {   /* one-time wake; repeated blanking resets the touch controller */
        write_file("/sys/class/graphics/fb0/blank", "1");
        msleep(40);
        write_file("/sys/class/graphics/fb0/blank", "0");
        display_inited = 1;
    }
}
static void text(int x, int y, const char *s, u32 c, int sc) {
    for (; *s; s++, x += (FW + 1) * sc) {
        unsigned ch = (unsigned char)*s;
        if (ch < 32 || ch > 126) ch = '?';
        const unsigned char *g = font_data[ch - 32];
        for (int yy = 0; yy < FH; yy++)
            for (int xx = 0; xx < FW; xx++)
                if (g[yy * FBPR + xx / 8] & (0x80 >> (xx % 8)))
                    for (int sy = 0; sy < sc; sy++) for (int sx = 0; sx < sc; sx++)
                        rect_back(x + xx * sc + sx, y + yy * sc + sy, 1, 1, c);
    }
}
static void ctext(int cx, int y, const char *s, u32 c, int sc) { text(cx - slen(s) * (FW + 1) * sc / 2, y, s, c, sc); }

/* ---------------------------------------------------------------- UI state */
enum { S_HOME, S_PAD, S_KEYS, S_STATUS, S_BRIGHT, S_DIAG };
static int screen = S_HOME;
static int sel = 0;                 /* highlighted button (hardware-key navigation) */
static long pwr_down_ms; static int pwr_handled;
static long tcnt, tpress; static long minx = 9999, maxx = -1, miny = 9999, maxy = -1; static int t_down;
static int bright = 120, screen_off = 0;
static char keylog[8][48]; static int keylog_n;
static int held_vup, held_vdn, held_pwr;
static int last_tx = -1, last_ty = -1;
static char tipline[64];
static long evcnt[10]; static int dbg_t, dbg_c, dbg_v, dbg_dev = -1; static long dbg_tx = -1, dbg_ty = -1; static long dbg_last_ms;
static long now_ms(void) { struct { long s; long ns; } ts; sc3(SYS_clock_gettime, 1, &ts, 0); return ts.s * 1000 + ts.ns / 1000000; }

struct btn { int x, y, w, h; const char *label; u32 col; };
static const struct btn home_tiles[6] = {
    { 40, 290, 340, 220, "TOUCH PAD", C_TILE },
    { 420, 290, 340, 220, "BUTTONS", C_TILE2 },
    { 40, 540, 340, 220, "STATUS", C_TILE3 },
    { 420, 540, 340, 220, "BRIGHTNESS", C_TILE4 },
    { 40, 790, 340, 220, "TOUCH DIAG", C_TILE2 },
    { 420, 790, 340, 220, "SCREEN OFF", C_TILE3 },
};
static const struct btn b_home = { 20, 20, 200, 80, "< HOME", C_PANEL };
static const struct btn b_clear = { 580, 20, 200, 80, "CLEAR", C_PANEL };
static const struct btn b_minus = { 60, 760, 300, 160, "-", C_TILE };
static const struct btn b_plus = { 440, 760, 300, 160, "+", C_TILE };

static int btn_count(void) { return screen == S_HOME ? 6 : screen == S_PAD ? 2 : screen == S_BRIGHT ? 3 : 1; }
static const struct btn *btn_get(int i) {
    switch (screen) {
        case S_HOME: return &home_tiles[i];
        case S_PAD: return i == 0 ? &b_home : &b_clear;
        case S_BRIGHT: return i == 0 ? &b_home : i == 1 ? &b_minus : &b_plus;
        default: return &b_home;
    }
}
static void draw_sel(void) {
    if (sel >= btn_count()) sel = 0;
    const struct btn *b = btn_get(sel); u32 c = RGB(255, 220, 60);
    rect_back(b->x - 6, b->y - 6, b->w + 12, 6, c); rect_back(b->x - 6, b->y + b->h, b->w + 12, 6, c);
    rect_back(b->x - 6, b->y, 6, b->h, c); rect_back(b->x + b->w, b->y, 6, b->h, c);
}
static void draw_btn(const struct btn *b) {
    rect_back(b->x, b->y, b->w, b->h, b->col);
    rect_back(b->x, b->y, b->w, 3, C_ACC);
    int sc = (b->h >= 200) ? 3 : 2;
    ctext(b->x + b->w / 2, b->y + (b->h - FH * sc) / 2, b->label, C_FG, sc);
}
static int hit(const struct btn *b, int x, int y) { return x >= b->x && x < b->x + b->w && y >= b->y && y < b->y + b->h; }

static void set_brightness(int v) {
    if (v < 0) v = 0; if (v > 255) v = 255;
    bright = v;
    char s[16]; itoa_(v, s);
    write_file("/sys/class/leds/lcd-backlight/brightness", s);
}

static int battery_pct(int *est) {
    char cap[16], vb[24]; *est = 0;
    read_file("/sys/class/power_supply/battery/capacity", cap, sizeof cap);
    long c = cap[0] ? atoi_(cap) : -1;
    if (c >= 0 && c <= 100) return (int)c;
    read_file("/sys/class/power_supply/battery/voltage_now", vb, sizeof vb);
    long v = vb[0] ? atoi_(vb) : 0;
    if (v > 100000) v /= 1000;            /* uV -> mV */
    if (v < 2500) return -1;
    long p = (v - 3300) * 100 / (4300 - 3300);
    if (p < 0) p = 0; if (p > 100) p = 100;
    *est = 1; return (int)p;
}
static void debug_footer(void) {
    rect_back(0, 1190, W, 90, C_BG);
    char l1[96], l2[96], t[24]; scpy(l1, "ev0-5: ");
    for (int i = 0; i < 6; i++) { itoa_(evcnt[i], t); scat(l1, t); scat(l1, " "); }
    scpy(l2, "last dev"); itoa_(dbg_dev, t); scat(l2, t); scat(l2, " t"); itoa_(dbg_t, t); scat(l2, t);
    scat(l2, " c"); itoa_(dbg_c, t); scat(l2, t); scat(l2, " v"); itoa_(dbg_v, t); scat(l2, t);
    scat(l2, " xy "); itoa_(dbg_tx, t); scat(l2, t); scat(l2, ","); itoa_(dbg_ty, t); scat(l2, t);
    text(20, 1200, l1, C_DIM, 1); text(20, 1232, l2, C_DIM, 1);
}
static void top_status(void) {
    char line[96], t[16], st[24]; int est;
    int p = battery_pct(&est);
    read_file("/sys/class/power_supply/battery/status", st, sizeof st);
    scpy(line, "Battery ");
    if (p < 0) scat(line, "?"); else { itoa_(p, t); scat(line, t); scat(line, est ? "% (est.)" : "%"); }
    scat(line, "  "); scat(line, st[0] ? st : "?");
    text(20, 1160, line, C_DIM, 1);
    debug_footer();
}
static long read_irq(void) {      /* sum of the TOUCH_PANEL interrupt counters in /proc/interrupts */
    static char buf[6000]; read_file("/proc/interrupts", buf, sizeof buf);
    char *p = buf;
    while (*p) {
        char *e = p; while (*e && *e != '\n') e++;
        int found = 0; for (char *q = p; q + 11 < e; q++) if (q[0] == 'T' && q[1] == 'O' && q[2] == 'U' && q[3] == 'C' && q[4] == 'H' && q[5] == '_') { found = 1; break; }
        if (found) {
            long sum = 0; char *q = p; while (q < e && *q != ':') q++; if (q < e) q++;
            while (q < e) { while (q < e && *q == ' ') q++; if (q < e && *q >= '0' && *q <= '9') { sum += atoi_(q); while (q < e && *q >= '0' && *q <= '9') q++; } else break; }
            return sum;
        }
        p = (*e) ? e + 1 : e;
    }
    return -1;
}
static void draw_diag(void) {
    rect_back(0, 0, W, H, C_BG);
    draw_btn(&b_home);
    ctext(W / 2, 140, "TOUCH DIAG", C_FG, 3);
    char l[96], t[24];
    scpy(l, "raw X: "); itoa_(dbg_tx, t); scat(l, t); scat(l, "   raw Y: "); itoa_(dbg_ty, t); scat(l, t); text(40, 230, l, C_FG, 2);
    scpy(l, "finger down: "); scat(l, t_down ? "YES" : "no"); text(40, 290, l, t_down ? C_OK : C_DIM, 2);
    scpy(l, "touch events: "); itoa_(tcnt, t); scat(l, t); scat(l, "   presses: "); itoa_(tpress, t); scat(l, t); text(40, 350, l, C_FG, 1);
    scpy(l, "controller IRQ count: "); itoa_(read_irq(), t); scat(l, t); text(40, 390, l, C_FG, 1);
    scpy(l, "X seen: "); itoa_(maxx < 0 ? -1 : minx, t); scat(l, t); scat(l, " .. "); itoa_(maxx, t); scat(l, t);
    scat(l, "     Y seen: "); itoa_(maxy < 0 ? -1 : miny, t); scat(l, t); scat(l, " .. "); itoa_(maxy, t); scat(l, t); text(40, 430, l, C_FG, 1);
    text(40, 480, "touch and drag around the screen: the cross", C_DIM, 1);
    text(40, 510, "follows the RAW coordinates. Corners should", C_DIM, 1);
    text(40, 540, "read about X 0..800  Y 0..1280.", C_DIM, 1);
    rect_back(10, 580, W - 20, 570, C_PANEL);
    if (dbg_tx >= 0 && dbg_ty >= 0) {
        int x = dbg_tx, y = 580 + (int)(dbg_ty * 570 / 1280);
        rect_back(x - 40, y - 2, 80, 4, C_ACC); rect_back(x - 2, y - 40, 4, 80, C_ACC);
    }
    debug_footer();
    draw_sel();
}
static void draw_home(void) {
    rect_back(0, 0, W, H, C_BG);
    ctext(W / 2, 90, "WALPAD 8G", C_FG, 4);
    ctext(W / 2, 220, "touch a tile  or  use the buttons", C_DIM, 1);
    for (int i = 0; i < 6; i++) draw_btn(&home_tiles[i]);
    ctext(W / 2, 1050, "VOL +/- move   POWER select   hold POWER: screen", C_DIM, 1);
    top_status();
    draw_sel();
}
static void draw_pad(void) {
    rect_back(0, 0, W, H, C_BG);
    draw_btn(&b_home); draw_btn(&b_clear);
    ctext(400, 130, "draw with your finger", C_DIM, 1);
    rect_back(10, 160, W - 20, H - 170, C_PANEL);
    draw_sel();
}
static void draw_keys(void) {
    rect_back(0, 0, W, H, C_BG);
    draw_btn(&b_home);
    ctext(W / 2, 140, "BUTTON TEST", C_FG, 3);
    struct btn v1 = { 40, 260, 220, 160, "VOL +", held_vup ? C_OK : C_PANEL };
    struct btn v2 = { 290, 260, 220, 160, "VOL -", held_vdn ? C_OK : C_PANEL };
    struct btn v3 = { 540, 260, 220, 160, "POWER", held_pwr ? C_OK : C_PANEL };
    draw_btn(&v1); draw_btn(&v2); draw_btn(&v3);
    text(40, 470, "recent events:", C_DIM, 1);
    for (int i = 0; i < 8; i++) {
        int idx = (keylog_n - 1 - i);
        if (idx < 0) break;
        text(40, 510 + i * 32, keylog[idx % 8], C_FG, 1);
    }
    text(40, 800, tipline, C_ACC, 1);
    debug_footer();
    draw_sel();
}
static void draw_status(void) {
    rect_back(0, 0, W, H, C_BG);
    draw_btn(&b_home);
    ctext(W / 2, 140, "STATUS", C_FG, 3);
    char v[40], line[96]; int y = 260;
#define ROW(label, path, unit) do { read_file(path, v, sizeof v); scpy(line, label); scat(line, v[0] ? v : "n/a"); scat(line, unit); text(40, y, line, C_FG, 1); y += 44; } while (0)
    { int est; int p = battery_pct(&est); char t[16]; itoa_(p, t); scpy(line, "battery:     "); scat(line, p < 0 ? "?" : t); scat(line, est ? " % (estimated from voltage)" : " %"); text(40, y, line, C_FG, 1); y += 44; }
    ROW("state:       ", "/sys/class/power_supply/battery/status", "");
    ROW("voltage uV:  ", "/sys/class/power_supply/battery/voltage_now", "");
    ROW("temp (0.1C): ", "/sys/class/power_supply/battery/temp", "");
    ROW("usb online:  ", "/sys/class/power_supply/usb/online", "");
    char up[48]; read_file("/proc/uptime", up, sizeof up);
    { long s = atoi_(up); char t[24]; itoa_(s / 60, t); scpy(line, "uptime:      "); scat(line, t); scat(line, " min"); text(40, y, line, C_FG, 1); y += 44; }
    { static char mi[1200]; read_file("/proc/meminfo", mi, sizeof mi);
      for (char *p = mi; *p; p++) if (p[0] == 'M' && p[1] == 'e' && p[2] == 'm' && p[3] == 'A' && p[4] == 'v') { p += 13; while (*p == ' ') p++; long kb = atoi_(p); char t[24]; itoa_(kb / 1024, t); scpy(line, "mem free:    "); scat(line, t); scat(line, " MB"); text(40, y, line, C_FG, 1); y += 44; break; } }
    { char t[16]; itoa_(bright, t); scpy(line, "brightness:  "); scat(line, t); text(40, y, line, C_FG, 1); }
    draw_sel();
}
static void draw_bright(void) {
    rect_back(0, 0, W, H, C_BG);
    draw_btn(&b_home);
    ctext(W / 2, 140, "BRIGHTNESS", C_FG, 3);
    char t[16]; itoa_(bright, t);
    ctext(W / 2, 400, t, C_FG, 6);
    rect_back(60, 600, W - 120, 60, C_PANEL);
    rect_back(60, 600, (W - 120) * bright / 255, 60, C_ACC);
    draw_btn(&b_minus); draw_btn(&b_plus);
    ctext(W / 2, 1000, "tap the bar, - / +, or VOL keys + POWER", C_DIM, 1);
    draw_sel();
}

static void show(int s) {
    screen = s;
    switch (s) {
        case S_HOME: draw_home(); break;
        case S_PAD: draw_pad(); break;
        case S_KEYS: draw_keys(); break;
        case S_STATUS: draw_status(); break;
        case S_BRIGHT: draw_bright(); break;
        case S_DIAG: draw_diag(); break;
    }
    blit_all(); commit();
}
static void go(int s) { sel = 0; if (s == S_DIAG) { minx = miny = 9999; maxx = maxy = -1; tcnt = tpress = 0; } show(s); }

static void logkey(int dev, int code, int val) {
    char *l = keylog[keylog_n % 8]; char t[16];
    scpy(l, "event"); itoa_(dev, t); scat(l, t); scat(l, " key "); itoa_(code, t); scat(l, t);
    scat(l, code == 115 ? " VOL+" : code == 114 ? " VOL-" : code == 116 ? " POWER" : "");
    scat(l, val == 1 ? " down" : val == 0 ? " up" : " repeat");
    keylog_n++;
}

static void set_screen_power(int on);
static void touch_tap(int x, int y) {
    switch (screen) {
        case S_HOME:
            for (int i = 0; i < 6; i++) if (hit(&home_tiles[i], x, y)) {
                if (i == 5) { set_screen_power(0); return; }
                go(i == 0 ? S_PAD : i == 1 ? S_KEYS : i == 2 ? S_STATUS : i == 3 ? S_BRIGHT : S_DIAG); return; }
            break;
        case S_PAD:
            if (hit(&b_home, x, y)) go(S_HOME); else if (hit(&b_clear, x, y)) show(S_PAD);
            break;
        case S_KEYS: case S_STATUS: case S_DIAG:
            if (hit(&b_home, x, y)) go(S_HOME);
            break;
        case S_BRIGHT:
            if (hit(&b_home, x, y)) { go(S_HOME); break; }
            if (hit(&b_minus, x, y)) { set_brightness(bright - 20); show(S_BRIGHT); }
            else if (hit(&b_plus, x, y)) { set_brightness(bright + 20); show(S_BRIGHT); }
            else if (y >= 580 && y < 680 && x >= 60 && x < W - 60) { set_brightness((x - 60) * 255 / (W - 120)); show(S_BRIGHT); }
            break;
    }
}
static void activate_sel(void) { const struct btn *b = btn_get(sel); touch_tap(b->x + b->w / 2, b->y + b->h / 2); }
static void paint(int x, int y) {
    if (y < 170 || y > H - 15) return;
    for (int j = -8; j <= 8; j++) for (int i = -8; i <= 8; i++) if (i * i + j * j <= 64) px(x + i, y + j, C_ACC);
}

static void set_screen_power(int on) {
    if (on) { write_file("/sys/class/graphics/fb0/blank", "0"); set_brightness(bright ? bright : 120); screen_off = 0; show(screen); }
    else { screen_off = 1; write_file("/sys/class/leds/lcd-backlight/brightness", "0"); write_file("/sys/class/graphics/fb0/blank", "1"); }
}

/* ---------------------------------------------------------------- main loop */
struct pfd { int fd; short events, revents; };
struct ievt { long sec, usec; u16 type, code; i32 value; };

static int cmain_(void) {
    long fbfd = sys_open("/dev/fb0", O_RDWR);
    if (fbfd >= 0) {
        long m = sc6(SYS_mmap, 0, (long)PAGE_BYTES * 3, 3, 1, fbfd, 0);
        if (m > 0 || m < -4096) fbmem = (u32 *)m;
    }
    set_brightness(bright);
    go(S_HOME);

    struct pfd fds[12]; int idx[12]; int nfd = 0;
    for (int i = 0; i < 10; i++) {
        char p[24]; scpy(p, "/dev/input/event"); char t[4]; itoa_(i, t); scat(p, t);
        long fd = sys_open(p, O_RDONLY | O_NONBLOCK);
        if (fd >= 0) { fds[nfd].fd = (int)fd; fds[nfd].events = 1; fds[nfd].revents = 0; idx[nfd] = i; nfd++; }
    }
    int tx = -1, ty = -1, down = 0, moved = 0, sx = -1, sy = -1;
    for (;;) {
        struct { long s; long ns; } to = { 2, 0 };
        if (held_pwr) { to.s = 0; to.ns = 100000000L; }
        for (int i = 0; i < nfd; i++) fds[i].revents = 0;
        long r = sc6(SYS_ppoll, (long)fds, nfd, (long)&to, 0, 8, 0);
        if (held_pwr && !pwr_handled && now_ms() - pwr_down_ms >= 1000) { pwr_handled = 1; set_screen_power(screen_off); }
        if (r == 0) {   /* periodic refresh */
            if (!screen_off && (screen == S_STATUS || screen == S_DIAG)) show(screen);
            continue;
        }
        for (int i = 0; i < nfd; i++) {
            if (!(fds[i].revents & 1)) continue;
            struct ievt e;
            while (sc3(SYS_read, fds[i].fd, &e, sizeof e) == (long)sizeof e) {
                int dev = idx[i];
                if (dev < 10) evcnt[dev]++;
                if (e.type != 0) { dbg_dev = dev; dbg_t = e.type; dbg_c = e.code; dbg_v = e.value; }
                if (e.type == 3 && (e.code == 53 || e.code == 0)) dbg_tx = e.value;
                if (e.type == 3 && (e.code == 54 || e.code == 1)) dbg_ty = e.value;
                if (e.type == 1 && (e.code == 114 || e.code == 115 || e.code == 116 || (e.code != 330 && e.code != 325 && e.code > 0 && e.code < 250))) {
                    logkey(dev, e.code, e.value);
                    if (e.code == 115) held_vup = e.value != 0;
                    if (e.code == 114) held_vdn = e.value != 0;
                    if (e.code == 116) held_pwr = e.value != 0;
                    if (e.code == 116) {
                        if (e.value == 1) { pwr_down_ms = now_ms(); pwr_handled = 0; }
                        else if (e.value == 0) {
                            if (!pwr_handled) { if (screen_off) set_screen_power(1); else activate_sel(); }
                            pwr_handled = 0;
                        }
                        continue;
                    }
                    if (screen_off) continue;
                    if ((e.code == 115 || e.code == 114) && e.value != 0) {
                        int n = btn_count(); sel = (sel + (e.code == 115 ? 1 : n - 1)) % n;
                        show(screen);
                        continue;
                    }
                    if (screen == S_KEYS) show(S_KEYS);
                } else if (dev == 4 || e.type == 3 || e.code == 330) {
                    if (screen_off) continue;
                    if (dev == 4 && e.type != 0) tcnt++;
                    if (e.type == 1 && e.code == 330) { t_down = e.value == 1; if (e.value == 1) tpress++; }
                    if (e.type == 3 && (e.code == 53 || e.code == 0)) { if (e.value < minx) minx = e.value; if (e.value > maxx) maxx = e.value; }
                    if (e.type == 3 && (e.code == 54 || e.code == 1)) { if (e.value < miny) miny = e.value; if (e.value > maxy) maxy = e.value; }
                    if (e.type == 3 && (e.code == 53 || e.code == 0)) { tx = e.value; moved = 1; }
                    else if (e.type == 3 && (e.code == 54 || e.code == 1)) { ty = e.value; moved = 1; }
                    else if (e.type == 3 && e.code == 57) { if (e.value < 0) { if (down && tx >= 0) touch_tap(sx >= 0 ? sx : tx, sy >= 0 ? sy : ty); down = 0; sx = sy = -1; } else down = 1; }
                    else if (e.type == 1 && e.code == 330) {
                        if (e.value == 1) down = 1;
                        else if (down) { touch_tap(sx >= 0 ? sx : tx, sy >= 0 ? sy : ty); down = 0; sx = sy = -1; }
                    }
                    else if (e.type == 0 && e.code == 0 && moved) {
                        moved = 0;
                        if (down && tx >= 0 && ty >= 0) {
                            if (sx < 0) { sx = tx; sy = ty; }
                            if (screen == S_PAD) { paint(tx, ty); commit(); }
                            last_tx = tx; last_ty = ty;
                        }
                    }
                }
            }
        }
        if (!screen_off && (screen == S_HOME || screen == S_KEYS || screen == S_STATUS || screen == S_DIAG)) {
            long n = now_ms();
            if (n - dbg_last_ms > 250) { dbg_last_ms = n; show(screen); }
        }
    }
    return 0;
}
int cmain(long *sp) { (void)sp; return cmain_(); }
__asm__(".global _start\n_start:\n mov x0, sp\n bl cmain\n mov x8, #93\n svc 0\n");
