/*
 * wpdprops.c — a stand-in for the two libcutils symbols the MediaTek
 * connectivity loaders need.
 *
 * wmt_loader publishes the CONNSYS chip id as an Android system property and
 * wmt_launcher spins in a sleep loop until it can read it back.  There is no
 * Android init on this firmware, so there is no property service, and the
 * handshake never completes — wmt_launcher never opens /dev/stpwmt, the ROM
 * patch request goes unanswered, and the radio refuses to power on.
 *
 * Both binaries reach the property store through libcutils' property_get /
 * property_set rather than bionic's __system_property_* directly, so
 * LD_PRELOADing these two definitions is enough: the linker puts preloaded
 * symbols ahead of libcutils in the lookup order.
 *
 * The store is a flat "key=value\n" file.  property_set appends and
 * property_get takes the last match, so a rewrite is just another append.
 *
 * Freestanding on purpose: raw syscalls only, no libc, so the same object
 * loads under bionic without caring which libc version is present.
 *
 * Build: tools/propshim32/build.sh
 *
 * propshim64b: the 64-bit twin (own file /tmp/wpd-props64, 256 KB buffer) of propshim32: the 32-bit ARM (EABI) twin of tools/propshim/wpdprops.c for the 32-bit MediaTek
 * modem daemons (ccci_mdinit, ccci_fsd, nvram_daemon).  Only the syscall numbers and the trap
 * sequence differ.  The store is /tmp/wpd-props64 (a different file from the Wi-Fi stack's /tmp/wpd-props, which the 8 KB 64-bit shim reads), and
 * the same properties.  tools/propshim itself is left untouched (AGENTS.md rule 4).
 */

#ifndef PROP_FILE
#define PROP_FILE "/tmp/wpd-props64"   /* its own file: never the Wi-Fi stack's /tmp/wpd-props */
#endif
#define PROP_VALUE_MAX 92
#ifndef BUFSZ
#define BUFSZ 262144   /* the build.prop files alone are ~20 KB: the old 8 KB limit hid later entries */
#endif

#define AT_FDCWD (-100)
#define O_RDONLY 0
#define O_WRONLY 1
#define O_CREAT 0100
#define O_APPEND 02000

/* ARM EABI syscall numbers */
#define SYS_openat 56
#define SYS_close 57
#define SYS_read 63
#define SYS_write 64

typedef unsigned long size_t;

static long syscall4(long n, long a, long b, long c, long d)
{
    register long x8 __asm__("x8") = n;
    register long x0 __asm__("x0") = a;
    register long x1 __asm__("x1") = b;
    register long x2 __asm__("x2") = c;
    register long x3 __asm__("x3") = d;
    __asm__ volatile("svc #0"
                     : "+r"(x0)
                     : "r"(x1), "r"(x2), "r"(x3), "r"(x8)
                     : "memory", "cc");
    return x0;
}

static int s_open(const char *path, int flags, int mode)
{
    return (int)syscall4(SYS_openat, AT_FDCWD, (long)path, flags, mode);
}
static long s_read(int fd, void *buf, size_t n)
{
    return syscall4(SYS_read, fd, (long)buf, (long)n, 0);
}
static long s_write(int fd, const void *buf, size_t n)
{
    return syscall4(SYS_write, fd, (long)buf, (long)n, 0);
}
static int s_close(int fd) { return (int)syscall4(SYS_close, fd, 0, 0, 0); }

static size_t s_strlen(const char *s)
{
    size_t n = 0;
    while (s && s[n]) n++;
    return n;
}

/* Read the whole store.  Returns the byte count, 0 if it does not exist. */
static char g_store[BUFSZ];
static size_t load(char *buf, size_t cap)
{
    int fd = s_open(PROP_FILE, O_RDONLY, 0);
    size_t got = 0;
    long n;

    if (fd < 0) return 0;
    while (got < cap - 1) {
        n = s_read(fd, buf + got, cap - 1 - got);
        if (n <= 0) break;
        got += (size_t)n;
    }
    s_close(fd);
    buf[got] = '\0';
    return got;
}

int property_get(const char *key, char *value, const char *default_value)
{
    char *buf = g_store;
    size_t klen = s_strlen(key);
    size_t len = load(buf, BUFSZ);
    const char *hit = 0;
    size_t i = 0;

    if (!value) return 0;
    value[0] = '\0';

    /* scan every line; the last match wins, so a later set overrides */
    while (i < len) {
        size_t start = i;
        while (i < len && buf[i] != '\n') i++;
        if (i - start > klen && buf[start + klen] == '=') {
            size_t j = 0;
            while (j < klen && buf[start + j] == key[j]) j++;
            if (j == klen) hit = buf + start + klen + 1;
        }
        if (i < len) buf[i] = '\0';
        i++;
    }

    if (!hit) {
        size_t n = 0;
        if (!default_value) return 0;
        while (default_value[n] && n < PROP_VALUE_MAX - 1) {
            value[n] = default_value[n];
            n++;
        }
        value[n] = '\0';
        return (int)n;
    }

    {
        size_t n = 0;
        while (hit[n] && n < PROP_VALUE_MAX - 1) {
            value[n] = hit[n];
            n++;
        }
        value[n] = '\0';
        return (int)n;
    }
}

int property_set(const char *key, const char *value)
{
    char line[256];
    size_t n = 0, i;
    int fd;

    for (i = 0; key[i] && n < sizeof(line) - 2; i++) line[n++] = key[i];
    line[n++] = '=';
    for (i = 0; value && value[i] && n < sizeof(line) - 2; i++) line[n++] = value[i];
    line[n++] = '\n';

    fd = s_open(PROP_FILE, O_WRONLY | O_CREAT | O_APPEND, 0644);
    if (fd < 0) return -1;
    s_write(fd, line, n);
    s_close(fd);
    return 0;
}
