#!/usr/bin/env python3
"""
wifi_survey.py — the WiFi slice of the device recon, on its own.

Thin front-end over device_recon.py so there is a single probe catalogue and a
single serial engine. Collects the 'wifi' and 'kernel' groups, which together
answer the one question the WiFi plan hinges on: does this kernel have the
driver, and can stock modules load into it?

Usage:
    scripts/wifi_survey.py            # results land in data/recon/
    scripts/wifi_survey.py --watch
"""
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import device_recon  # noqa: E402

if __name__ == "__main__":
    argv = sys.argv[1:]
    if "--only" not in argv:
        argv += ["--only", "wifi", "kernel"]
    sys.argv = [sys.argv[0]] + argv
    sys.exit(device_recon.main())
