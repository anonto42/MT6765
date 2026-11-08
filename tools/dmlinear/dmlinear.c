/* dmlinear NAME BACKING_PATH START_SECTOR NUM_SECTORS
 * Create a read-only dm-linear device and a /dev/mapper/NAME block node.
 * Freestanding aarch64 Linux, raw syscalls, no libc. */
typedef unsigned long u64; typedef unsigned int u32; typedef int i32;

static long sc(long n, long a, long b, long c, long d) {
    register long x8 __asm__("x8") = n;
    register long x0 __asm__("x0") = a;
    register long x1 __asm__("x1") = b;
    register long x2 __asm__("x2") = c;
    register long x3 __asm__("x3") = d;
    __asm__ volatile("svc 0" : "+r"(x0) : "r"(x8), "r"(x1), "r"(x2), "r"(x3) : "memory", "cc");
    return x0;
}
#define SYS_ioctl 29
#define SYS_mknodat 33
#define SYS_openat 56
#define SYS_write 64
#define SYS_exit 93
#define AT_FDCWD (-100)

struct dm_ioctl {
    u32 version[3]; u32 data_size; u32 data_start; u32 target_count;
    i32 open_count; u32 flags; u32 event_nr; u32 padding; u64 dev;
    char name[128]; char uuid[129]; char data[7];
};
struct dm_target_spec { u64 sector_start; u64 length; i32 status; u32 next; char target_type[16]; };

#define DM_IOCTL_REQ(cmd) (0xC0000000UL | ((unsigned long)sizeof(struct dm_ioctl) << 16) | (0xfdUL << 8) | (cmd))
#define DM_DEV_CREATE 3
#define DM_DEV_SUSPEND 6
#define DM_TABLE_LOAD 9
#define DM_READONLY_FLAG 1u

static char buf[8192] __attribute__((aligned(8)));

static void zero(void *p, int n) { char *c = p; while (n--) *c++ = 0; }
static int slen(const char *s) { int n = 0; while (s[n]) n++; return n; }
static void scopy(char *d, const char *s) { while ((*d++ = *s++)); }
static u64 num(const char *s) { u64 v = 0; while (*s >= '0' && *s <= '9') v = v * 10 + (*s++ - '0'); return v; }
static void say(const char *s) { sc(SYS_write, 2, (long)s, slen(s), 0); }
static void utoa(u64 v, char *o) { char t[24]; int i = 0; if (!v) t[i++] = '0'; while (v) { t[i++] = '0' + v % 10; v /= 10; } int j = 0; while (i) o[j++] = t[--i]; o[j] = 0; }

static void init_hdr(struct dm_ioctl *h, const char *name, u32 flags) {
    zero(h, sizeof *h);
    h->version[0] = 4; h->version[1] = 0; h->version[2] = 0;
    h->data_size = sizeof *h; h->data_start = sizeof *h; h->flags = flags;
    scopy(h->name, name);
}

int cmain(long *sp) {
    long argc = sp[0]; char **argv = (char **)(sp + 1);
    if (argc < 5) { say("usage: dmlinear NAME PATH START NSECT\n"); return 2; }
    const char *name = argv[1], *path = argv[2];
    u64 start = num(argv[3]), nsect = num(argv[4]);
    long ctl = sc(SYS_openat, AT_FDCWD, (long)"/dev/mapper/control", 2, 0);
    if (ctl < 0) { say("dmlinear: open control failed\n"); return 3; }
    struct dm_ioctl *h = (struct dm_ioctl *)buf;

    init_hdr(h, name, DM_READONLY_FLAG);
    if (sc(SYS_ioctl, ctl, DM_IOCTL_REQ(DM_DEV_CREATE), (long)h, 0) < 0) { say("dmlinear: DEV_CREATE failed\n"); return 4; }

    init_hdr(h, name, DM_READONLY_FLAG);
    h->target_count = 1;
    struct dm_target_spec *t = (struct dm_target_spec *)(buf + sizeof *h);
    zero(t, sizeof *t);
    t->sector_start = 0; t->length = nsect;
    scopy(t->target_type, "linear");
    char *params = (char *)(t + 1);
    int n = 0; scopy(params, path); n = slen(params); params[n++] = ' ';
    utoa(start, params + n); n += slen(params + n); params[n++] = 0;
    int plen = (n + 7) & ~7;
    t->next = sizeof *t + plen;
    h->data_size = sizeof *h + sizeof *t + plen;
    if (sc(SYS_ioctl, ctl, DM_IOCTL_REQ(DM_TABLE_LOAD), (long)h, 0) < 0) { say("dmlinear: TABLE_LOAD failed\n"); return 5; }

    init_hdr(h, name, 0);
    if (sc(SYS_ioctl, ctl, DM_IOCTL_REQ(DM_DEV_SUSPEND), (long)h, 0) < 0) { say("dmlinear: RESUME failed\n"); return 6; }

    char node[160]; scopy(node, "/dev/mapper/"); scopy(node + 12, name);
    if (sc(SYS_mknodat, AT_FDCWD, (long)node, 0060600, (long)h->dev) < 0) { say("dmlinear: mknod failed\n"); return 7; }
    say("dmlinear: ok "); say(name); say("\n");
    return 0;
}
__asm__(".global _start\n_start:\n mov x0, sp\n bl cmain\n mov x8, #93\n svc 0\n");
