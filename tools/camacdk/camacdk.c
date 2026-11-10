/*
 * camacdk: still/preview capture through MediaTek's ACDK (libacdk.so), without the Android camera
 * HAL.  A bionic aarch64 executable that is run through the Android bootstrap linker:
 *
 *   linker64 camacdk capture main /root/x.jpg
 *
 * Everything it knows about the ACDK API was recovered from /vendor/bin/factory (the camera test)
 * and libacdk.so itself; see docs/31-camera-acdk.md (function addresses, confidence levels).
 *
 *   camacdk list                       camera_info + a probe of the main and the sub sensor
 *   camacdk capture <main|sub> <out.jpg>
 *   camacdk preview <main|sub> <outdir> <n>   n preview frames (YUY2 + PPM)
 *
 * Freestanding style (like tools/propshim): no NDK, no headers.  Our own minimal types and libc
 * prototypes; clang + lld only.  The entry point is hand-written the way bionic's crtbegin does it
 * (call __libc_init), so libc works normally in main().
 *
 * Defensive: every phase runs under alarm(); a hung ACDK call ends the process with exit 124 and a
 * log line.  Exit codes: 0 ok, 1 usage, 2 libacdk missing, 3 open, 4 set source, 5 init,
 * 6 preview, 7 capture produced nothing, 8 cannot write output, 124 timeout.
 *
 * Build: tools/camacdk/build.sh   (output data/camera/camacdk)
 */

typedef unsigned char u8;
typedef unsigned short u16;
typedef unsigned int u32;
typedef unsigned long u64;
typedef long i64;
typedef unsigned long size_t;
typedef long ssize_t;
typedef __builtin_va_list va_list;
#define va_start(a, b) __builtin_va_start(a, b)
#define va_end(a) __builtin_va_end(a)
#define NULL ((void *)0)

/* ---- libc / libdl, resolved at run time by the bionic linker (DT_NEEDED libc.so, libdl.so) ---- */
extern int open(const char *, int, ...);
extern int close(int);
extern ssize_t read(int, void *, size_t);
extern ssize_t write(int, const void *, size_t);
extern int rename(const char *, const char *);
extern int unlink(const char *);
extern int mkdir(const char *, u32);
extern int stat(const char *, void *);
extern unsigned int alarm(unsigned int);
extern void (*signal(int, void (*)(int)))(int);
extern int usleep(unsigned int);
extern void _exit(int);
extern long syscall(long, ...);
extern int vsnprintf(char *, size_t, const char *, va_list);
extern void *malloc(size_t);
extern void free(void *);
extern char *getenv(const char *);
extern int fork(void);
extern int waitpid(int, int *, int);
extern int clock_gettime(int, void *);
extern void *dlopen(const char *, int);
extern void *dlsym(void *, const char *);
extern char *dlerror(void);
extern void __libc_init(void *raw_args, void (*onexit)(void), int (*slingshot)(int, char **, char **), void *structors);

#define O_RDONLY 0
#define O_WRONLY 1
#define O_CREAT 0100
#define O_TRUNC 01000
#define O_APPEND 02000
#define O_DIRECTORY 040000
#define RTLD_NOW 2
#define SIGALRM 14
#define SYS_getdents64 61

/* ---- ACDK ABI (recovered from the binaries; docs/31-camera-acdk.md) ---- */
#define ACDK_CMD_PREVIEW_START 0x80001001u
#define ACDK_CMD_PREVIEW_STOP 0x80001002u
#define ACDK_CMD_CAPTURE 0x80001003u
#define ACDK_CMD_SET_SRC_DEV 0x80001006u

#define SENSOR_DEV_MAIN 1
#define SENSOR_DEV_SUB 2
#define ACDK_CAP_FORMAT_JPEG 0x40
#define ACDK_BYPASS_FAKE_SURFACE 0x4 /* MDK_OpenBypass(flag): bit 2 selects fakeSurfaceView (no fb0, no overlay) */

typedef struct { /* ACDK_FEATURE_INFO_STRUCT, 32 bytes: MDK_IOControl loads it as ldp x2,x4 / ldp w3,w5 / ldr x6 */
    void *pIn;
    void *pOut;
    u32 inLen;
    u32 outLen;
    u32 *pRealOutLen;
} acdk_feat_t;

typedef struct { /* ACDK_PREVIEW_STRUCT, 40 bytes (the inLen the factory passes) */
    u32 width;      /*  0 */
    u32 height;     /*  4 */
    u16 flag8;      /*  8: the factory passes 1 (stored in AcdkMain+472) */
    u16 hdr;        /* 10: "Preview HDREn" */
    u32 pad12;      /* 12 */
    void *cb;       /* 16: preview callback; the factory passes NULL and the library never calls it */
    u32 two;        /* 24: the factory passes 2 (not read by sendcommand) */
    u32 opType;     /* 28: "Preview OP Type", 0 = normal */
    u32 pad32[2];   /* 32 */
} acdk_prv_t;

typedef struct { /* ACDK_CAPTURE_STRUCT, 56 bytes */
    u32 mode;       /*  0: "Capture Mode", the factory passes 1 */
    u32 opMode;     /*  4: "Operation Mode", manual capture passes 2 */
    u32 format;     /*  8: output format; 0x40 = JPEG (hardware JpgEnc); valid: 4 5 8 16 32 64 */
    u16 width;      /* 12: 0 = "use original capture size" */
    u16 height;     /* 14 */
    void *cb;       /* 16: callback(void *), called from inside the blocking capture call */
    u32 count;      /* 24: "cap cnt" */
    u32 isSave;     /* 28: "isSave" */
    u32 pad32;      /* 32 */
    u32 mfll;       /* 36: "MFLL_En" */
    u16 hdr;        /* 40: "mHDR_En" */
    u16 pad42;
    u32 multiNr;    /* 44: "eMultiNR" */
    u64 pad48;      /* 48 */
} acdk_cap_t;

typedef struct { /* what the capture callback receives (JPEG case, filled by AcdkMain::takePicture) */
    u32 type;       /*  0: = capture format (0x40 JPEG) */
    u32 pad4;
    u8 *addr;       /*  8: JPEG bitstream (valid only during the callback) */
    u32 width;      /* 16 */
    u32 height;     /* 20 */
    u32 size;       /* 24: size of the bitstream BUFFER, not the encoded length; trim at EOI */
} acdk_cb_t;

_Static_assert(sizeof(acdk_feat_t) == 32, "feature info");
_Static_assert(sizeof(acdk_prv_t) == 40, "preview struct");
_Static_assert(sizeof(acdk_cap_t) == 56, "capture struct");

typedef int (*fn_open_t)(void);
typedef int (*fn_openbypass_t)(u32);
typedef int (*fn_void_t)(void);
typedef int (*fn_ioctl_t)(u32, acdk_feat_t *);

static struct {
    void *lib;
    fn_open_t open_;
    fn_openbypass_t openbypass;
    fn_void_t init, deinit, close_;
    fn_ioctl_t ioctl_;
} A;

/* ---- tiny helpers ---- */
static size_t slen(const char *s) { size_t n = 0; while (s[n]) n++; return n; }
static int seq(const char *a, const char *b) { while (*a && *a == *b) { a++; b++; } return *a == *b; }
static int sprefix(const char *s, const char *p) { while (*p) { if (*s++ != *p++) return 0; } return 1; }
static void mcopy(void *d, const void *s, size_t n) { u8 *a = d; const u8 *b = s; while (n--) *a++ = *b++; }
static void mzero(void *d, size_t n) { u8 *a = d; while (n--) *a++ = 0; }

static const char *g_phase = "start";

static void lg(const char *fmt, ...)
{
    char b[512];
    va_list ap;
    int n;
    va_start(ap, fmt);
    n = vsnprintf(b, sizeof b - 1, fmt, ap);
    va_end(ap);
    if (n < 0) return;
    if (n > (int)sizeof b - 2) n = sizeof b - 2;
    b[n++] = '\n';
    write(2, "camacdk: ", 9);
    write(2, b, (size_t)n);
}

static void on_alarm(int sig)
{
    (void)sig;
    lg("TIMEOUT in phase '%s'", g_phase);
    _exit(124);
}

static void phase(const char *name, unsigned secs)
{
    g_phase = name;
    alarm(secs);
    lg("phase %s (limit %us)", name, secs);
}

static unsigned envnum(const char *name, unsigned dflt)
{
    const char *v = getenv(name);
    unsigned n = 0;
    if (!v || !*v) return dflt;
    while (*v >= '0' && *v <= '9') n = n * 10 + (unsigned)(*v++ - '0');
    return n;
}

static i64 now_ms(void)
{
    i64 ts[2];
    clock_gettime(0 /* CLOCK_REALTIME */, ts);
    return ts[0] * 1000 + ts[1] / 1000000;
}

static int write_all(int fd, const void *buf, size_t n)
{
    const u8 *p = buf;
    while (n) {
        ssize_t w = write(fd, p, n);
        if (w <= 0) return -1;
        p += w;
        n -= (size_t)w;
    }
    return 0;
}

/* write via a temp file and rename, so a reader never sees half a JPEG */
static int save_file(const char *path, const void *buf, size_t n)
{
    char tmp[512];
    int fd;
    if (slen(path) > sizeof tmp - 8) return -1;
    mcopy(tmp, path, slen(path) + 1);
    mcopy(tmp + slen(path), ".tmp", 5);
    fd = open(tmp, O_WRONLY | O_CREAT | O_TRUNC, 0644);
    if (fd < 0) return -1;
    if (write_all(fd, buf, n) < 0) { close(fd); unlink(tmp); return -1; }
    close(fd);
    return rename(tmp, path);
}

/* whole file into a malloc'd buffer (caller frees); *n = size; NULL on error */
static u8 *slurp(const char *path, size_t *n, size_t max)
{
    u8 st[128];
    size_t size, got = 0;
    u8 *buf;
    int fd;
    if (stat(path, st) != 0) return NULL;
    size = *(u64 *)(st + 48); /* aarch64 struct stat: st_size at 48, st_mtime at 88 */
    if (size == 0 || size > max) return NULL;
    buf = malloc(size);
    if (!buf) return NULL;
    fd = open(path, O_RDONLY);
    if (fd < 0) { free(buf); return NULL; }
    while (got < size) {
        ssize_t r = read(fd, buf + got, size - got);
        if (r <= 0) break;
        got += (size_t)r;
    }
    close(fd);
    if (got != size) { free(buf); return NULL; }
    *n = size;
    return buf;
}

static i64 mtime_of(const char *path)
{
    u8 st[128];
    if (stat(path, st) != 0) return -1;
    return *(i64 *)(st + 88);
}

/* Android property file read by the LD_PRELOADed shim (libwpdprops.so): "key=value\n", last wins */
static void set_prop(const char *key, const char *val)
{
    const char *f = getenv("CAMACDK_PROPS");
    char line[160];
    int fd, n = 0;
    size_t i;
    if (!f || !*f) f = "/tmp/wpd-props";
    for (i = 0; key[i] && n < 100; i++) line[n++] = key[i];
    line[n++] = '=';
    for (i = 0; val[i] && n < 150; i++) line[n++] = val[i];
    line[n++] = '\n';
    fd = open(f, O_WRONLY | O_CREAT | O_APPEND, 0644);
    if (fd < 0) { lg("cannot append to property file %s", f); return; }
    write_all(fd, line, (size_t)n);
    close(fd);
}

/* ---- libacdk ---- */
static void *need(const char *sym, int optional)
{
    void *p = dlsym(A.lib, sym);
    if (!p && !optional) lg("missing symbol %s: %s", sym, dlerror());
    return p;
}

static int load_acdk(void)
{
    const char *names[3] = { "libacdk.so", "/vendor/lib64/libacdk.so", NULL };
    int i;
    for (i = 0; names[i] && !A.lib; i++) {
        A.lib = dlopen(names[i], RTLD_NOW);
        if (!A.lib) lg("dlopen(%s) failed: %s", names[i], dlerror());
    }
    if (!A.lib) return -1;
    A.open_ = (fn_open_t)need("MDK_Open", 0);
    A.openbypass = (fn_openbypass_t)need("MDK_OpenBypass", 1);
    A.init = (fn_void_t)need("MDK_Init", 0);
    A.deinit = (fn_void_t)need("MDK_DeInit", 0);
    A.close_ = (fn_void_t)need("MDK_Close", 0);
    A.ioctl_ = (fn_ioctl_t)need("MDK_IOControl", 0);
    if (!A.open_ || !A.init || !A.deinit || !A.close_ || !A.ioctl_) return -1;
    return 0;
}

/* MDK_IOControl returns 1 whenever the device is open and the code is in range, whatever the command
 * did (AcdkIF.cpp discards sendcommand's result), so 0 here means "not opened / bad code" only. */
static int ctl(u32 cmd, void *in, u32 inLen, void *out, u32 outLen)
{
    acdk_feat_t f;
    u32 real = 0;
    mzero(&f, sizeof f);
    f.pIn = in; f.pOut = out; f.inLen = inLen; f.outLen = outLen; f.pRealOutLen = &real;
    return A.ioctl_(cmd, &f);
}

static int g_opened;

static unsigned bypass_flag(void) { return envnum("CAMACDK_BYPASS", ACDK_BYPASS_FAKE_SURFACE); }

static int session_open(int dev, unsigned bypass)
{
    int srcdev = dev;
    int ok;
    phase("open", envnum("CAMACDK_T_OPEN", 30));
    ok = (bypass && A.openbypass) ? A.openbypass(bypass) : A.open_();
    if (!ok) { lg("MDK_Open%s failed", bypass ? "Bypass" : ""); return 3; }
    g_opened = 1;
    phase("set_src_dev", 20);
    if (!ctl(ACDK_CMD_SET_SRC_DEV, &srcdev, 4, NULL, 0)) { lg("SET_SRC_DEV(%d) rejected", dev); return 4; }
    phase("init", envnum("CAMACDK_T_INIT", 60));
    if (!A.init()) { lg("MDK_Init failed (sensor %d not found, or an ACDK init error)", dev); return 5; }
    lg("sensor %d open", dev);
    return 0;
}

static void session_close(void)
{
    if (!g_opened) return;
    phase("close", 30);
    ctl(ACDK_CMD_PREVIEW_STOP, NULL, 0, NULL, 0); /* a "redundent command" is harmless */
    A.deinit();
    A.close_();
    g_opened = 0;
    alarm(0);
}

static int preview_start(u32 w, u32 h)
{
    acdk_prv_t p;
    mzero(&p, sizeof p);
    p.width = w; p.height = h; p.flag8 = 1; p.two = 2; p.opType = 0;
    phase("preview_start", envnum("CAMACDK_T_PREVIEW", 30));
    if (!ctl(ACDK_CMD_PREVIEW_START, &p, sizeof p, NULL, 0)) { lg("PREVIEW_START rejected"); return 6; }
    return 0;
}

static void sleep_ms(unsigned ms)
{
    while (ms >= 100) { usleep(100000); ms -= 100; }
    if (ms) usleep(ms * 1000);
}

static int dev_of(const char *s)
{
    if (seq(s, "main")) return SENSOR_DEV_MAIN;
    if (seq(s, "sub")) return SENSOR_DEV_SUB;
    return 0;
}

/* ---- capture ---- */
static u8 *g_jpeg;
static size_t g_jpeg_n;
static volatile int g_cb_calls;

/* SOI must be at 0; the EOI (FF D9) closes the stream, the rest of the buffer is padding */
static size_t jpeg_trim(const u8 *p, size_t n)
{
    size_t i;
    if (n < 4 || p[0] != 0xFF || p[1] != 0xD8) return 0;
    for (i = n; i >= 2; i--)
        if (p[i - 2] == 0xFF && p[i - 1] == 0xD9) return i;
    return 0;
}

static void cap_cb(void *arg)
{
    acdk_cb_t *c = arg;
    g_cb_calls++;
    if (!c) { lg("capture callback with NULL"); return; }
    lg("capture callback: type=0x%x %ux%u buffer=%u addr=%p", c->type, c->width, c->height, c->size, (void *)c->addr);
    if (c->type != ACDK_CAP_FORMAT_JPEG || !c->addr || c->size == 0 || c->size > (64u << 20)) return;
    if (g_jpeg) free(g_jpeg);
    g_jpeg = malloc(c->size);
    if (!g_jpeg) return;
    mcopy(g_jpeg, c->addr, c->size);
    g_jpeg_n = c->size;
}

static int cmd_capture(const char *which, const char *out)
{
    int dev = dev_of(which), rc;
    acdk_cap_t c;
    i64 t0 = now_ms() / 1000;
    size_t n;
    if (!dev) { lg("sensor must be main or sub"); return 1; }
    mkdir("/data/vendor", 0771); /* libacdk also writes /data/vendor/acdkCap.jpg itself */
    set_prop("vendor.camera.acdkdump.enable", "0");
    rc = session_open(dev, bypass_flag());
    if (rc) goto done;
    rc = preview_start(envnum("CAMACDK_PW", 320), envnum("CAMACDK_PH", 240));
    if (rc) goto done;
    phase("settle", 60);
    sleep_ms(envnum("CAMACDK_SETTLE_MS", 2500)); /* AE/AWB/AF converge while the preview runs */
    mzero(&c, sizeof c);
    c.mode = 1; c.opMode = 2; c.format = ACDK_CAP_FORMAT_JPEG; c.cb = (void *)cap_cb;
    c.count = 1; c.isSave = envnum("CAMACDK_SAVE", 1);
    phase("capture", envnum("CAMACDK_T_CAPTURE", 45));
    ctl(ACDK_CMD_CAPTURE, &c, sizeof c, NULL, 0);
    lg("capture returned, callbacks=%d", g_cb_calls);
    n = g_jpeg ? jpeg_trim(g_jpeg, g_jpeg_n) : 0;
    if (!n) { /* fallback: the library's own copy, written with the exact encoded length */
        size_t fn = 0;
        u8 *f = NULL;
        if (mtime_of("/data/vendor/acdkCap.jpg") >= t0) f = slurp("/data/vendor/acdkCap.jpg", &fn, 64u << 20);
        if (f && jpeg_trim(f, fn)) {
            lg("using /data/vendor/acdkCap.jpg (%u bytes)", (unsigned)fn);
            if (g_jpeg) free(g_jpeg);
            g_jpeg = f; g_jpeg_n = fn; n = jpeg_trim(f, fn);
        } else if (f) free(f);
    }
    if (!n) { lg("no JPEG came back (callbacks=%d)", g_cb_calls); rc = 7; goto done; }
    phase("write", 20);
    if (save_file(out, g_jpeg, n) < 0) { lg("cannot write %s", out); rc = 8; goto done; }
    lg("OK %s %u bytes", out, (unsigned)n);
    rc = 0;
done:
    session_close();
    return rc;
}

/* ---- preview: frames come from libacdk's own dump (vendor.camera.acdkdump.enable=2 writes each
 * preview frame to /data/vendor/acdkPrv2_<w>x<h>_<stride>.yuy2, overwriting the same file) ---- */
static u32 take_num(const char **p)
{
    u32 v = 0;
    while (**p >= '0' && **p <= '9') v = v * 10 + (u32)(*(*p)++ - '0');
    return v;
}

static int find_dump(char *path, size_t cap, u32 *w, u32 *h, u32 *stride)
{
    char buf[4096];
    long got;
    int fd = open("/data/vendor", O_RDONLY | O_DIRECTORY);
    int found = 0;
    if (fd < 0) return 0;
    while (!found && (got = syscall(SYS_getdents64, fd, buf, sizeof buf)) > 0) {
        long off = 0;
        while (off < got && !found) {
            u16 reclen = *(u16 *)(buf + off + 16);
            const char *name = buf + off + 19; /* linux_dirent64: d_name at 19 */
            if (sprefix(name, "acdkPrv2_") && slen(name) + 14 < cap) {
                const char *p = name + 9;
                u32 a = take_num(&p), b = 0, c = 0;
                if (*p == 'x') { p++; b = take_num(&p); }
                if (*p == '_') { p++; c = take_num(&p); }
                if (a && b) {
                    mcopy(path, "/data/vendor/", 13);
                    mcopy(path + 13, name, slen(name) + 1);
                    *w = a; *h = b; *stride = c;
                    found = 1;
                }
            }
            if (reclen == 0) break;
            off += reclen;
        }
    }
    close(fd);
    return found;
}

static u8 clamp8(int v) { return (u8)(v < 0 ? 0 : v > 255 ? 255 : v); }

static int put_num(char *d, u32 v)
{
    char t[12];
    int n = 0, k = 0;
    do { t[n++] = (char)('0' + v % 10); v /= 10; } while (v);
    while (n) d[k++] = t[--n];
    return k;
}

/* YUY2 (Y0 U Y1 V) to a binary PPM, BT.601 full range; rows are `stride` bytes apart */
static int yuy2_to_ppm(const u8 *y, u32 w, u32 h, u32 stride, const char *path)
{
    char hdr[40];
    u8 *rgb;
    size_t o;
    u32 r, x;
    int hl = 0, rc;
    hdr[hl++] = 'P'; hdr[hl++] = '6'; hdr[hl++] = '\n';
    hl += put_num(hdr + hl, w); hdr[hl++] = ' ';
    hl += put_num(hdr + hl, h);
    hdr[hl++] = '\n'; hdr[hl++] = '2'; hdr[hl++] = '5'; hdr[hl++] = '5'; hdr[hl++] = '\n';
    rgb = malloc((size_t)w * h * 3 + 64);
    if (!rgb) return -1;
    mcopy(rgb, hdr, (size_t)hl);
    o = (size_t)hl;
    for (r = 0; r < h; r++) {
        const u8 *row = y + (size_t)r * stride;
        for (x = 0; x + 1 < w; x += 2) {
            int y0 = row[x * 2], u = row[x * 2 + 1] - 128, y1 = row[x * 2 + 2], v = row[x * 2 + 3] - 128;
            int rr = (359 * v) >> 8, gg = (88 * u + 183 * v) >> 8, bb = (454 * u) >> 8;
            rgb[o++] = clamp8(y0 + rr); rgb[o++] = clamp8(y0 - gg); rgb[o++] = clamp8(y0 + bb);
            rgb[o++] = clamp8(y1 + rr); rgb[o++] = clamp8(y1 - gg); rgb[o++] = clamp8(y1 + bb);
        }
    }
    rc = save_file(path, rgb, o);
    free(rgb);
    return rc;
}

static int cmd_preview(const char *which, const char *outdir, unsigned frames)
{
    int dev = dev_of(which), rc, got = 0;
    char dump[256], dst[300];
    u32 w = 0, h = 0, stride = 0;
    i64 last_mtime = -1, deadline, last_ms = 0;
    if (!dev) { lg("sensor must be main or sub"); return 1; }
    if (frames == 0 || frames > 999 || slen(outdir) > 200) { lg("n must be 1..999 and outdir short"); return 1; }
    mkdir(outdir, 0755);
    mkdir("/data/vendor", 0771);
    set_prop("vendor.camera.acdkdump.enable", "2");
    rc = session_open(dev, bypass_flag());
    if (rc) goto done;
    rc = preview_start(envnum("CAMACDK_PW", 320), envnum("CAMACDK_PH", 240));
    if (rc) goto done;
    phase("frames", 20 + frames * 3);
    deadline = now_ms() + (i64)(10 + frames * 2) * 1000;
    while (got < (int)frames && now_ms() < deadline) {
        i64 mt;
        size_t fn = 0, blen;
        u8 *f;
        u32 st;
        sleep_ms(120);
        if (!find_dump(dump, sizeof dump, &w, &h, &stride)) continue;
        mt = mtime_of(dump);
        f = slurp(dump, &fn, 64u << 20);
        if (!f) continue;
        /* the dump file is rewritten in place: take it when it changed (mtime has 1 s resolution,
         * so also take it when 400 ms passed) */
        if (got > 0 && mt == last_mtime && now_ms() - last_ms < 400) { free(f); continue; }
        last_mtime = mt;
        last_ms = now_ms();
        st = stride >= w * 2 ? stride : w * 2;
        blen = slen(outdir);
        mcopy(dst, outdir, blen);
        dst[blen++] = '/';
        mcopy(dst + blen, "prv", 3);
        dst[blen + 3] = (char)('0' + got / 100 % 10);
        dst[blen + 4] = (char)('0' + got / 10 % 10);
        dst[blen + 5] = (char)('0' + got % 10);
        mcopy(dst + blen + 6, ".yuy2", 6);
        if (save_file(dst, f, fn) < 0) { lg("cannot write %s", dst); free(f); rc = 8; goto done; }
        if (fn >= (size_t)st * h) {
            mcopy(dst + blen + 6, ".ppm", 5);
            yuy2_to_ppm(f, w, h, st, dst);
        }
        lg("frame %d: %ux%u stride %u, %u bytes", got, w, h, st, (unsigned)fn);
        free(f);
        got++;
    }
    if (!got) { lg("no preview frame appeared in /data/vendor (acdkdump.enable=2 needs the property shim)"); rc = 6; }
    else if (got < (int)frames) { lg("only %d of %u frames", got, frames); rc = 0; }
    else rc = 0;
done:
    session_close();
    set_prop("vendor.camera.acdkdump.enable", "0");
    return rc;
}

/* ---- list ---- */
static void cat_file(const char *path)
{
    char b[1024];
    ssize_t r;
    int fd = open(path, O_RDONLY);
    if (fd < 0) { lg("%s: not readable", path); return; }
    while ((r = read(fd, b, sizeof b)) > 0) write(1, b, (size_t)r);
    close(fd);
}

/* one sensor in a child process, so a crash or a hang there cannot take the listing down */
static int probe_one(int dev)
{
    int pid = fork();
    if (pid == 0) {
        int rc = session_open(dev, bypass_flag());
        session_close();
        _exit(rc);
    }
    if (pid > 0) {
        int st = 0;
        waitpid(pid, &st, 0);
        if ((st & 0x7f) == 0) return (st >> 8) & 0xff;
        return 100 + (st & 0x7f); /* killed by a signal */
    }
    return 99;
}

static void say_probe(const char *name, int rc)
{
    char b[64];
    int n = 0;
    while (*name) b[n++] = *name++;
    b[n++] = ':'; b[n++] = ' ';
    if (rc == 0) { b[n++] = 'o'; b[n++] = 'k'; }
    else { const char *s = "FAILED (exit "; while (*s) b[n++] = *s++; n += put_num(b + n, (u32)rc); b[n++] = ')'; }
    b[n++] = '\n';
    write(1, b, (size_t)n);
}

static int cmd_list(void)
{
    int rm, rs;
    write(1, "== /proc/driver/camera_info\n", 28);
    cat_file("/proc/driver/camera_info");
    rm = probe_one(SENSOR_DEV_MAIN);
    rs = probe_one(SENSOR_DEV_SUB);
    say_probe("main", rm);
    say_probe("sub", rs);
    return (rm == 0 || rs == 0) ? 0 : 3;
}

static int usage(void)
{
    static const char u[] =
        "usage: camacdk list\n"
        "       camacdk capture <main|sub> <out.jpg>\n"
        "       camacdk preview <main|sub> <outdir> <n>\n"
        "env: CAMACDK_SETTLE_MS CAMACDK_PW CAMACDK_PH CAMACDK_BYPASS CAMACDK_SAVE CAMACDK_PROPS CAMACDK_T_*\n";
    write(2, u, sizeof u - 1);
    return 1;
}

static int cam_main(int argc, char **argv, char **envp)
{
    int rc;
    (void)envp;
    signal(SIGALRM, on_alarm);
    if (argc < 2) return usage();
    phase("dlopen", 30);
    if (load_acdk() != 0) return 2;
    if (seq(argv[1], "list") && argc == 2) rc = cmd_list();
    else if (seq(argv[1], "capture") && argc == 4) rc = cmd_capture(argv[2], argv[3]);
    else if (seq(argv[1], "preview") && argc == 5) {
        const char *s = argv[4];
        rc = cmd_preview(argv[2], argv[3], take_num(&s));
    } else return usage();
    alarm(0);
    return rc;
}

/* crtbegin equivalent: __libc_init(raw_args, onexit, main, structors).  The arrays are
 * [-1, handlers..., 0] like the linker-generated ones in /vendor/bin/factory (.preinit_array 0x10). */
static void *g_pre[2] = { (void *)-1, 0 };
static void *g_init[2] = { (void *)-1, 0 };
static void *g_fini[2] = { (void *)-1, 0 };
static void *g_structors[3] = { g_pre, g_init, g_fini };

void cam_start_c(void *raw_args)
{
    __libc_init(raw_args, 0, cam_main, g_structors);
    for (;;) _exit(127);
}

__asm__(".text\n.globl _start\n.type _start,%function\n_start:\n"
        "    mov x0, sp\n"
        "    and sp, x0, #-16\n"
        "    bl cam_start_c\n"
        "    brk #0\n");
