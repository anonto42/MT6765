/* fbmirror pause logic: pure functions, no libc, no syscalls.  Included by fbmirror.c (freestanding aarch64)
 * and by test_logic.c (host compiler).  While status.json says "screen":"off" fbmirror stops comparing/copying
 * frames; it resumes within one poll and then does one full refresh. */
#ifndef FBMIRROR_LOGIC_H
#define FBMIRROR_LOGIC_H

#define GATE_POLL_MS 250        /* how often status.json is read while copying (paused: every loop, see below) */
#define GATE_RUN_SLEEP_MS 40    /* normal loop sleep */
#define GATE_PAUSE_SLEEP_MS 200 /* loop sleep while the screen is off (<= GATE_POLL_MS, so resume latency is one poll) */
#define GATE_STALE_S 10         /* status.json is written every 2 s; older (or from the future) = not trusted */

/* Returns 1 only when the JSON text has a key "screen" whose string value is exactly "off".  Anything else
 * (on, dim, key missing, truncated, not a string) returns 0 = keep copying. */
static int fb_parse_screen_off(const char *s, long n) {
    for (long i = 0; i + 8 <= n; i++) {
        if (s[i] != '"' || s[i + 1] != 's' || s[i + 2] != 'c' || s[i + 3] != 'r' || s[i + 4] != 'e' ||
            s[i + 5] != 'e' || s[i + 6] != 'n' || s[i + 7] != '"') continue;
        long j = i + 8;
        while (j < n && (s[j] == ' ' || s[j] == '\t' || s[j] == '\n' || s[j] == '\r')) j++;
        if (j >= n || s[j] != ':') continue;   /* "screen" used as a value, not a key */
        j++;
        while (j < n && (s[j] == ' ' || s[j] == '\t' || s[j] == '\n' || s[j] == '\r')) j++;
        if (j + 5 <= n && s[j] == '"' && s[j + 1] == 'o' && s[j + 2] == 'f' && s[j + 3] == 'f' && s[j + 4] == '"') return 1;
        return 0;
    }
    return 0;
}

/* Off only if the text says off AND the file is fresh (mtime within GATE_STALE_S of now, either direction). */
static int fb_status_off(const char *s, long n, long mtime, long now) {
    long age = now - mtime;
    if (age > GATE_STALE_S || age < -GATE_STALE_S) return 0;
    return fb_parse_screen_off(s, n);
}

typedef struct { int paused; int refresh; long since_ms; } fbgate;

/* Call once per loop with the milliseconds slept since the previous call.  Returns 1 when status.json must be read. */
static int gate_poll_due(fbgate *g, long elapsed_ms) {
    g->since_ms += elapsed_ms;
    if (g->paused || g->since_ms >= GATE_POLL_MS) { g->since_ms = 0; return 1; }
    return 0;
}

/* Feed the result of a status read (1 = screen off).  Leaving the paused state asks for one full refresh. */
static void gate_update(fbgate *g, int off) {
    if (off && !g->paused) g->paused = 1;
    else if (!off && g->paused) { g->paused = 0; g->refresh = 1; }
}

static long gate_sleep_ms(const fbgate *g) { return g->paused ? GATE_PAUSE_SLEEP_MS : GATE_RUN_SLEEP_MS; }

#endif
