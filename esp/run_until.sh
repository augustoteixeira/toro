#!/usr/bin/env bash
# run_until.sh — flash the ESP32, then read serial output until a sentinel
# string appears, an error pattern matches, or a timeout is reached.
#
# The flash and serial monitor are separate steps to avoid the "Failed to
# initialize input reader" error that espflash --monitor produces when there
# is no interactive terminal.
#
# Usage:
#   ./run_until.sh <sentinel> [--error <pattern>] [--timeout <N>] [--poll <N>]
#                              [--port <device>] [--baud <rate>] [--features <f>]
#
# Arguments:
#   sentinel    String to wait for in the serial output (signals success).
#
# Options:
#   --error <pattern>   Regex that signals failure (default: "error|panic|failed").
#   --timeout <N>       Max seconds to wait before giving up (default: 60).
#   --poll <N>          Seconds between output checks (default: 2).
#   --port <device>     Serial device (default: /dev/ttyUSB0).
#   --baud <rate>       Baud rate (default: 115200).
#   --features <f>      Cargo features to enable (e.g. "test-mode").
#
# Examples:
#   ./run_until.sh "BOOT_OK"
#   ./run_until.sh "temperature: " --timeout 120
set -euo pipefail

if [ $# -lt 1 ]; then
    echo "Usage: $0 <sentinel> [--error <pattern>] [--timeout <N>] [--poll <N>] [--port <device>] [--baud <rate>]" >&2
    exit 1
fi

SENTINEL="$1"
shift

ERROR_PATTERN="error|panic|failed"
MAX_WAIT=60
POLL=2
PORT="/dev/ttyUSB0"
BAUD=115200
FEATURES=""

while [[ $# -gt 0 ]]; do
    case "$1" in
        --error)    ERROR_PATTERN="$2"; shift 2 ;;
        --timeout)  MAX_WAIT="$2";      shift 2 ;;
        --poll)     POLL="$2";          shift 2 ;;
        --port)     PORT="$2";          shift 2 ;;
        --baud)     BAUD="$2";          shift 2 ;;
        --features) FEATURES="$2";      shift 2 ;;
        *) echo "Unknown option: $1" >&2; exit 1 ;;
    esac
done

FEATURES_ARG=""
[ -n "$FEATURES" ] && FEATURES_ARG="--features $FEATURES"

LOG=$(mktemp)
SERIAL_LOG=$(mktemp)

cleanup() {
    [ -n "${SERIAL_PID:-}" ] && kill "$SERIAL_PID" 2>/dev/null || true
}
trap cleanup EXIT

# Step 1: Flash (exits on its own). espflash resets the device after flashing.
echo "Flashing..."
if ! cargo espflash flash --port "$PORT" $FEATURES_ARG >"$LOG" 2>&1; then
    echo "Flash failed. Log:" >&2
    cat "$LOG" >&2
    exit 1
fi
echo "Flash complete. Waiting for sentinel \"$SENTINEL\"..."

# Step 2: Start serial reader right after flash completes. The device has just
#         been reset by espflash and is booting, so we catch the output from the
#         beginning of the boot sequence.
stty -F "$PORT" "$BAUD" raw -echo 2>/dev/null || true
cat "$PORT" >"$SERIAL_LOG" 2>&1 &
SERIAL_PID=$!

# Step 3: Poll serial output for sentinel / error / timeout.
elapsed=0
while true; do
    if grep -q "$SENTINEL" "$SERIAL_LOG"; then
        echo "Done: sentinel \"$SENTINEL\" found."
        echo "Serial log: $SERIAL_LOG"
        exit 0
    fi
    if grep -Eqi "$ERROR_PATTERN" "$SERIAL_LOG"; then
        echo "Failed: error pattern matched in serial output. Log:" >&2
        cat "$SERIAL_LOG" >&2
        exit 1
    fi
    sleep "$POLL"
    elapsed=$((elapsed + POLL))
    if [ "$elapsed" -ge "$MAX_WAIT" ]; then
        echo "Timed out after ${MAX_WAIT}s. Serial log:" >&2
        cat "$SERIAL_LOG" >&2
        exit 1
    fi
done
