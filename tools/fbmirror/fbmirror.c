/* fbmirror - copy a (rotated) part of an Xvfb screen (XWD file from `Xvfb -fbdir DIR`) to the MediaTek fb0.
 * The X screen is 1280x1280; the visible area is 800x1280 (rotation 0/180) or 1280x800 (90/270), rotated onto
 * the native 800x1280 panel.  Rotation comes from /run/walpad/orientation (written by walpad-devd).
 * Freestanding aarch64 Linux, raw syscalls.  Only writes (and pans) when the picture changed.
 *   fbmirror /run/walpad/xfb/Xvfb_screen0
 * Panel: fb0 800x1280 BGRA, 3 pages; present with sysfs pan "0,1280" then "0,0"; wake once with blank 1->0. */
typedef unsigned long u64; typedef long i64; typedef unsigned int u32; typedef unsigned char u8;
#define W 800
#define H 1280
#define PAGE_PX (W * H)
#define PAGE_BYTES (PAGE_PX * 4)
#include "logic.h"

static long sc6(long n, long a, long b, long c, long d, long e, long f) {
    register long x8 __asm__("x8") = n; register long x0 __asm__("x0") = a; register long x1 __asm__("x1") = b;
    register long x2 __asm__("x2") = c; register long x3 __asm__("x3") = d; register long x4 __asm__("x4") = e; register long x5 __asm__("x5") = f;
    __asm__ volatile("svc 0" : "+r"(x0) : "r"(x8), "r"(x1), "r"(x2), "r"(x3), "r"(x4), "r"(x5) : "memory", "cc");
    return x0;
}
#define sc3(n, a, b, c) sc6(n, (long)(a), (long)(b), (long)(c), 0, 0, 0)
#define SYS_openat 56
#define SYS_close 57
#define SYS_read 63
#define SYS_write 64
#define SYS_fstat 80
#define SYS_nanosleep 101
#define SYS_clock_gettime 113
#define SYS_mmap 222
#define AT_FDCWD (-100)
void *memcpy(void *d, const void *s, unsigned long n) { u8 *a = d; const u8 *b = s; while (n--) { *a++ = *b++; __asm__ volatile(""); } return d; }
void *memset(void *d, int c, unsigned long n) { u8 *a = d; while (n--) { *a++ = (u8)c; __asm__ volatile(""); } return d; }
static int read_ori(void) {
    char b[8]; long fd = sc3(SYS_openat, AT_FDCWD, "/run/walpad/orientation", 0);
    if (fd < 0) return 0;
    long r = sc3(SYS_read, fd, b, 7); sc3(SYS_close, fd, 0, 0);
    if (r <= 0) return 0;
    int v = 0; for (long i = 0; i < r && b[i] >= '0' && b[i] <= '9'; i++) v = v * 10 + (b[i] - '0');
    return (v == 90 || v == 180 || v == 270) ? v : 0;
}
/* 1 = status.json is fresh and says "screen":"off"; 0 = anything else (missing, stale, unparsable: keep copying) */
static int read_screen_off(void) {
    char b[2048]; u64 st[18];   /* aarch64 struct stat is 128 bytes; st_mtime.tv_sec is at byte 88 */
    long fd = sc3(SYS_openat, AT_FDCWD, "/run/walpad/status.json", 0);
    if (fd < 0) return 0;
    long r = -1;
    if (sc3(SYS_fstat, fd, st, 0) == 0) r = sc3(SYS_read, fd, b, sizeof b);
    sc3(SYS_close, fd, 0, 0);
    if (r <= 0) return 0;
    long ts[2] = { 0, 0 };
    if (sc3(SYS_clock_gettime, 0, ts, 0) != 0) return 0;
    return fb_status_off(b, r, (long)st[11], ts[0]);
}
static int slen(const char *s) { int n = 0; while (s[n]) n++; return n; }
static void write_file(const char *p, const char *s) { long fd = sc3(SYS_openat, AT_FDCWD, p, 1); if (fd < 0) return; sc3(SYS_write, fd, s, slen(s)); sc3(SYS_close, fd, 0, 0); }
static void msleep(long ms) { struct { long s, ns; } ts = { ms / 1000, (ms % 1000) * 1000000L }; sc3(SYS_nanosleep, &ts, 0, 0); }
static u32 shadow[PAGE_PX];
static u32 be32(const u8 *p) { return ((u32)p[0] << 24) | ((u32)p[1] << 16) | ((u32)p[2] << 8) | p[3]; }
static void say(const char *s) { sc3(SYS_write, 2, s, slen(s)); }
static long map(const char *path, long len, int prot, int flags, int oflag) {
    long fd = sc3(SYS_openat, AT_FDCWD, path, oflag);
    if (fd < 0) return fd;
    return sc6(SYS_mmap, 0, len, prot, flags, fd, 0);
}

int cmain(long *sp) {
    long argc = sp[0]; char **argv = (char **)(sp + 1);
    const char *xfile = argc > 1 ? argv[1] : "/run/walpad/xfb/Xvfb_screen0";
    long fbm = map("/dev/fb0", (long)PAGE_BYTES * 3, 3, 1, 2);
    if (fbm < 0 && fbm > -4096) { say("fbmirror: cannot map /dev/fb0\n"); return 2; }
    u32 *fb = (u32 *)fbm;
    u8 hdr[128]; long fd = -1;
    for (int i = 0; i < 600; i++) { fd = sc3(SYS_openat, AT_FDCWD, xfile, 0); if (fd >= 0) { long r = sc3(SYS_read, fd, hdr, sizeof hdr); if (r >= 100) break; sc3(SYS_close, fd, 0, 0); fd = -1; } msleep(100); }
    if (fd < 0) { say("fbmirror: no Xvfb framebuffer file\n"); return 3; }
    u32 hsize = be32(hdr + 0), pw = be32(hdr + 16), ph = be32(hdr + 20), bpp = be32(hdr + 44), bpl = be32(hdr + 48), ncol = be32(hdr + 76);
    u64 off = hsize + (u64)ncol * 12;
    if (pw < 1280 || ph < 1280 || bpp != 32) { say("fbmirror: X screen must be at least 1280x1280, 32bpp\n"); return 4; }
    long stride = bpl / 4;
    long xm = sc6(SYS_mmap, 0, (long)(off + (u64)bpl * ph), 1, 1, fd, 0);
    if (xm < 0 && xm > -4096) { say("fbmirror: cannot map XWD file\n"); return 5; }
    const u32 *src = (const u32 *)((u8 *)xm + off);
    int inited = 0, woke = 0, cur = 0, ori = -1, tick = 0;
    fbgate gate = { 0, 0, 0 }; long slept = 0;
    for (;;) {
        if (gate_poll_due(&gate, slept)) gate_update(&gate, read_screen_off());
        if (gate.paused) { slept = gate_sleep_ms(&gate); msleep(slept); continue; }   /* screen off: no compare, no copy */
        if (gate.refresh) { gate.refresh = 0; inited = 0; tick = 0; for (int i = 0; i < PAGE_PX; i++) shadow[i] = 0; }   /* resumed: one full refresh, re-read rotation */
        if ((tick++ & 3) == 0) { int o = read_ori(); if (o != ori) { int first = ori < 0; ori = o; for (int i = 0; i < PAGE_PX; i++) shadow[i] = 0; if (!first) msleep(300); /* let X relayout, then draw once; never blank on rotation */ } }
        int changed = 0; u32 *sh = shadow;
        if (ori == 0) {
            for (int y = 0; y < H; y++) { const u32 *r = src + (long)y * stride; for (int x = 0; x < W; x++) { u32 v = r[x] | 0xFF000000u; if (sh[x] != v) { sh[x] = v; changed = 1; } } sh += W; }
        } else if (ori == 180) {
            for (int y = 0; y < H; y++) { const u32 *r = src + (long)(H - 1 - y) * stride; for (int x = 0; x < W; x++) { u32 v = r[W - 1 - x] | 0xFF000000u; if (sh[x] != v) { sh[x] = v; changed = 1; } } sh += W; }
        } else if (ori == 90) {   /* logical 1280x800, lx = py, ly = 799 - px */
            for (int y = 0; y < H; y++) { const u32 *c = src + y; for (int x = 0; x < W; x++) { u32 v = c[(long)(W - 1 - x) * stride] | 0xFF000000u; if (sh[x] != v) { sh[x] = v; changed = 1; } } sh += W; }
        } else {                  /* 270: lx = 1279 - py, ly = px */
            for (int y = 0; y < H; y++) { const u32 *c = src + (H - 1 - y); for (int x = 0; x < W; x++) { u32 v = c[(long)x * stride] | 0xFF000000u; if (sh[x] != v) { sh[x] = v; changed = 1; } } sh += W; }
        }
        if (changed || !inited) {
            int nxt = cur ^ 1;   /* real double buffering: draw into the hidden page, flip once */
            u64 *d = (u64 *)(fb + (u64)nxt * PAGE_PX); const u64 *c = (const u64 *)shadow;
            for (long i = 0; i < PAGE_BYTES / 8; i++) d[i] = c[i];
            write_file("/sys/class/graphics/fb0/pan", nxt ? "0,1280" : "0,0");
            cur = nxt;
            if (!woke) { write_file("/sys/class/graphics/fb0/blank", "1"); msleep(40); write_file("/sys/class/graphics/fb0/blank", "0"); woke = 1; }
            inited = 1;
        }
        slept = gate_sleep_ms(&gate); msleep(slept);
    }
    return 0;
}
__asm__(".global _start\n_start:\n mov x0, sp\n bl cmain\n mov x8, #93\n svc 0\n");
