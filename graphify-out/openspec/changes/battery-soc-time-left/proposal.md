# Smooth battery percent and time left

## Why

Owner request 2026-11-11: the battery percent jumps up and down when brightness or load changes (it is the raw voltage), and the owner wants the estimated time the tablet will last.

## What changes

- walpad-core `logic/soc.rs`: IR-compensated, filtered OCV plus coulomb counting estimator; shown percent monotonic in the charge direction, slew-limited, persisted in `/var/lib/walpad/battery.json`.
- Time left / time to full from smoothed current and capacity (`battery_mah` in devd.json, else charge_full, else 3000).
- walpad-proto Battery gains optional `time_left_min`, `time_to_full_min`, `time_calculating`.
- Shell: Quick Settings battery row and Device battery card show the time.

## Out of scope

Gauge initialisation, learning real capacity, 4.35 V cell tables, a settings UI for `battery_mah`.

## Device risk

None: read-only sysfs; one small file under /var/lib/walpad. No flashing. Guard (low/critical shutdown) stays on raw voltage.

## Secrets impact

None.

## Verification

Unit tests with simulated traces (core), proto compatibility test, shell goldens. On the tablet: change brightness and watch the percent hold; check the time row after 2 minutes; reboot and see the percent resume.
