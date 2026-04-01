# Agent Instructions

## Core Principles

- Make **atomic changes only**. One small, focused change at a time.
- Do **not** go beyond what was explicitly requested. If a task says "add X", add X and nothing else.
- Do **not** refactor, rename, reorganize, or "improve" anything that was not part of the request.
- Do **not** add dependencies, files, or boilerplate unless explicitly asked.
- Ask for clarification before making any assumption that would affect the implementation.

## Project Overview

This is a DIY weather station project with two components:

1. **ESP32 firmware** — written in Rust, using `esp-idf` with FreeRTOS for async support and verified TLS.
2. **Web server** — written in Rust using the Rocket framework, serving the web frontend.

## Development Approach

- Progress is **intentionally slow and deliberate**.
- Each change must be justified by an explicit instruction.
- Prefer correctness and clarity over cleverness or completeness.
- Do not anticipate future steps or scaffold ahead.

## Working Dynamics

- The user drives all decisions; agents propose and wait for confirmation.
- When a task requires interactive commands (e.g. `cargo generate`), instruct the user to run them and wait for the result before proceeding.
- Diagnose problems by actually running commands and reading output — do not guess.
- To flash the device, use `esp/run_until.sh <sentinel>` instead of running `cargo espflash flash --monitor` directly. The script runs the flash command as a background process and exits cleanly once the sentinel string appears in the serial output. Example: `./run_until.sh "BOOT_OK"`. See `esp/run_until.sh` for full usage.
- Known environment issues to be aware of:
  - `cargo` is not on the default `PATH`; prefix commands with `export PATH="$HOME/.cargo/bin:$PATH"` or rely on the shell having it set.
  - `cargo-espflash` 4.x fails to compile; use version 3.3.0.
  - `libclang-dev` and `libudev-dev` must be installed via `apt` before building the esp crate.
  - `ldproxy` must be installed via `cargo install ldproxy`.
  - The user must be in the `dialout` group to flash over USB.
