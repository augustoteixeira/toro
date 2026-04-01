#!/usr/bin/env bash
# run_until.sh — flash the ESP32 and exit when a sentinel string appears in the
# serial output, an error pattern matches, or a timeout is reached.
#
# Usage:
#   ./run_until.sh <sentinel> [--error <pattern>] [--timeout <N>] [--poll <N>]
#
# Arguments:
#   sentinel    String to wait for in the output (signals success).
#
# Options:
#   --error <pattern>   Regex that signals failure (default: "error|panic|failed").
#   --timeout <N>       Max seconds to wait before giving up (default: 60).
#   --poll <N>          Seconds between output checks (default: 2).
#
# Examples:
#   ./run_until.sh "BOOT_OK"
#   ./run_until.sh "temperature: " --timeout 120
set -euo pipefail

if [ $# -lt 1 ]; then
    echo "Usage: $0 <sentinel> [--error <pattern>] [--timeout <N>] [--poll <N>]" >&2
    exit 1
fi

SENTINEL="$1"
shift

ERROR_PATTERN="error|panic|failed"
MAX_WAIT=60
POLL=2

while [[ $# -gt 0 ]]; do
    case "$1" in
        --error)   ERROR_PATTERN="$2"; shift 2 ;;
        --timeout) MAX_WAIT="$2";      shift 2 ;;
        --poll)    POLL="$2";          shift 2 ;;
        *) echo "Unknown option: $1" >&2; exit 1 ;;
    esac
done

LOG=$(mktemp)
echo "Output log: $LOG"

cargo espflash flash --monitor >"$LOG" 2>&1 &
PID=$!

elapsed=0
while kill -0 "$PID" 2>/dev/null; do
    if grep -q "$SENTINEL" "$LOG"; then
        kill "$PID" 2>/dev/null
        echo "Done: sentinel \"$SENTINEL\" found."
        exit 0
    fi
    if grep -Eqi "$ERROR_PATTERN" "$LOG"; then
        kill "$PID" 2>/dev/null
        echo "Failed: error pattern matched. Log:" >&2
        cat "$LOG" >&2
        exit 1
    fi
    sleep "$POLL"
    elapsed=$((elapsed + POLL))
    if [ "$elapsed" -ge "$MAX_WAIT" ]; then
        kill "$PID" 2>/dev/null
        echo "Timed out after ${MAX_WAIT}s. Log:" >&2
        cat "$LOG" >&2
        exit 1
    fi
done

# Process exited on its own; wait ensures output is fully flushed before grepping
wait "$PID" 2>/dev/null || true
if grep -q "$SENTINEL" "$LOG"; then
    echo "Done: sentinel \"$SENTINEL\" found."
    exit 0
else
    echo "Process exited without sentinel. Log:" >&2
    cat "$LOG" >&2
    exit 1
fi
