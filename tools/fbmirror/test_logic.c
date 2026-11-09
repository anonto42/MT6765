/* Host test for logic.h (compiled with the host compiler, run in docker by test.sh). */
#include <stdio.h>
#include <string.h>
#include "logic.h"

static int fails;
#define CHECK(c) do { if (!(c)) { printf("FAIL line %d: %s\n", __LINE__, #c); fails++; } } while (0)
static int off(const char *s) { return fb_parse_screen_off(s, (long)strlen(s)); }

int main(void) {
    CHECK(off("{\"screen\":\"off\"}") == 1);
    CHECK(off("{\"screen\": \"off\", \"brightness\": 3}") == 1);
    CHECK(off("{\"time\":1,\"screen\" :\n \"off\"}") == 1);
    CHECK(off("{\"screen\":\"on\"}") == 0);
    CHECK(off("{\"screen\":\"dim\"}") == 0);
    CHECK(off("{\"screen\":\"offx\"}") == 0);
    CHECK(off("{\"screen\":\"of") == 0);          /* truncated */
    CHECK(off("{\"screen\":") == 0);
    CHECK(off("{\"screen\"") == 0);
    CHECK(off("{\"screen\":off}") == 0);          /* not a string */
    CHECK(off("") == 0);
    CHECK(off("garbage \"off\"") == 0);
    CHECK(off("{\"wifi\":\"screen\",\"screen\":\"off\"}") == 1);   /* "screen" as a value is skipped */
    CHECK(off("{\"wifi\":\"screen\",\"x\":\"off\"}") == 0);
    CHECK(off("{\"screen_x\":\"off\"}") == 0);
    CHECK(fb_parse_screen_off("{\"screen\":\"off\"}", 0) == 0);

    const char *j = "{\"screen\":\"off\"}"; long n = (long)strlen(j);
    CHECK(fb_status_off(j, n, 1000, 1002) == 1);
    CHECK(fb_status_off(j, n, 1000, 1000 + GATE_STALE_S) == 1);
    CHECK(fb_status_off(j, n, 1000, 1000 + GATE_STALE_S + 1) == 0);   /* stale: keep copying */
    CHECK(fb_status_off(j, n, 1000, 1000 - GATE_STALE_S - 1) == 0);   /* clock jumped */
    CHECK(fb_status_off("{\"screen\":\"on\"}", 15, 1000, 1000) == 0);

    /* state machine: running polls every GATE_POLL_MS, paused every loop, resume does one refresh */
    fbgate g = {0, 0, 0};
    CHECK(gate_sleep_ms(&g) == GATE_RUN_SLEEP_MS);
    int polls = 0;
    for (int i = 0; i < 25; i++) polls += gate_poll_due(&g, GATE_RUN_SLEEP_MS);   /* 1000 ms: a poll every 7 loops = 280 ms */
    CHECK(polls == 3);
    gate_update(&g, 0); CHECK(!g.paused && !g.refresh);
    gate_update(&g, 1); CHECK(g.paused && !g.refresh);
    CHECK(gate_sleep_ms(&g) == GATE_PAUSE_SLEEP_MS);
    CHECK(GATE_PAUSE_SLEEP_MS <= 250);
    CHECK(gate_poll_due(&g, GATE_PAUSE_SLEEP_MS) == 1);                          /* paused: poll every loop */
    gate_update(&g, 1); CHECK(g.paused && !g.refresh);
    gate_update(&g, 0); CHECK(!g.paused && g.refresh == 1);                      /* resume: refresh requested once */
    g.refresh = 0; gate_update(&g, 0); CHECK(!g.refresh);
    gate_update(&g, 1); gate_update(&g, 0); CHECK(g.refresh == 1);               /* off again, on again */

    puts(fails ? "FAIL" : "PASS fbmirror logic");
    return fails != 0;
}
