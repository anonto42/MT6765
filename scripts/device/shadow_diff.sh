#!/bin/sh
# M2.3 wrapper: shadow_diff.sh PYTHON_LOG... --core CORE_LOG [--tz-offset-min N]   (runs on the host)
exec python3 -I "$(dirname "$0")/shadow_diff.py" "$@"
