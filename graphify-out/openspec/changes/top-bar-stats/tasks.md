## 1. Core and contract
- [x] 1.1 walpad-proto: `StatsStatus`, `StatsItems`, `stats_config`; CONTRACTS.md
- [x] 1.2 walpad-core `stats` module: cpu/meminfo/net parsers, 2 s sampler, persisted switches
- [x] 1.3 Parser unit tests and IPC tests; walpad-ctl `stats`

## 2. Shell
- [x] 2.1 Header layout (pure, never overlaps the clock or the pill), render, redraw rules
- [x] 2.2 Settings > Top bar items page with four switches
- [x] 2.3 Snapshot goldens (portrait and landscape), app tests

## 3. Check
- [ ] 3.1 `scripts/check.sh` prints ALL CHECKS PASSED

## 4. Device (needs the tablet; no flashing)
- [ ] 4.1 Figures match `top`, `free`, `df /`, a download; both orientations; items off = no sampling
