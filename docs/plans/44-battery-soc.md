# Battery percent and time left (owner request 2026-11-11)

Problem: the MediaTek gauge is not initialised (`capacity` = -1), so the percent came from the
instantaneous terminal voltage, which sags under load (brightness, CPU) and rebounds, so the percent
wobbled up and down. Goal: a calm, phone-like percent plus an estimated time left.

## Research summary (short)

Without a working gauge the usual practice is: (1) an open-circuit-voltage (OCV) to SoC table, valid only
for a rested cell; (2) correct the terminal voltage for load with the internal resistance,
`V_ocv = V + I*R` while discharging (minus while charging); (3) coulomb counting (integrating the
current over the capacity) for short-term smoothness, corrected by the OCV value because counting alone
drifts and OCV alone is noisy under load; (4) charge and discharge OCV curves differ slightly
(hysteresis), and capacity changes with temperature and age, so it must be treated as a hint. Phones
add a presentation layer on top: the shown percent only moves in the direction of the charge state,
changes at a limited rate, is persisted across reboots and is forced to 100 at charge termination
(Android shows the fuel gauge's UI SoC, which is rated against design capacity and not measured
continuously).

Sources (searched 2026-11-11):
- Coulomb counting with OCV correction, tabular OCV with charge/discharge tables: Frontiers in Energy
  Research 10.3389/fenrg.2025.1724766; Energies 2022 tabular OCV-SOC paper (scholar.uwindsor.ca/mechanicalengpub/31)
- Equivalent-circuit (resistance) model vs coulomb counting benchmark: EPE 2025 paper 0164 (vbn.aau.dk)
- Android assumes rated capacity and estimates the percent from voltage/current behaviour; health
  figures are estimates: AOSP BatteryService commits mirrored by e/OS; arXiv 1711.03651
The rate limits and constants below are engineering choices for this tablet, not taken from a standard.

## Design (crates/walpad-core/src/logic/soc.rs)

- `V_ocv = V -/+ |I| * R_int` (sign from the charge state, not from the sensor sign); R_int from the
  gauge `ptim_resist_get` (milliohm) when 50..800, else 0.2 ohm. Filtered with a 180 s EMA, mapped
  through a generic 4.2 V Li-ion table.
- The estimate is moved by coulomb counting (`current_now` over capacity) and pulled towards the table
  value with a 30 min time constant, at most 12 points per hour.
- Shown percent: one point per 60 s at most; never rises while discharging, never falls while charging;
  `Full` forces 100; charging never shows 100 before `Full`. A working gauge (0..100) would win.
- Boot: seeded from `/var/lib/walpad/battery.json` (`{percent, saved_unix}`, written at most every 5
  min when the value changed and on the power-off/reboot path) when it is within 25 points of the
  rested voltage; otherwise the voltage wins. Garbage, out of range, future or older than 7 days
  is ignored (an unset clock accepts it).
- The low/critical shutdown guard still uses the raw voltage (logic/battery.rs `Guard`).
- Time left = percent * capacity / smoothed discharge current (5 min EMA; spikes above 4x and
  currents outside 20 mA..4 A are ignored; needs 2 min of data, else "calculating..."; hidden above
  72 h). Time to full = remaining / smoothed charge current with a taper (above 80 % at half the
  current). Both are estimates: UI shows "~", rounded to 5 min.
- Capacity: `battery_mah` in `/etc/walpad/devd.json` (read the mAh printed on the battery label and set
  `"battery_mah": 4000` in that file; there is no UI or ctl setting for it), else `charge_full` when >= 1500 mAh,
  else 3000 mAh.

## Limits

The 4.2 V table is generic; if the cell is a 4.35 V type the shown percent reaches about 95 % before
`Full` and then jumps to 100 at termination. Units of `current_now` (uA) and `ptim_resist_get` (mohm) are
assumed. Not verified on the tablet over a full discharge.
