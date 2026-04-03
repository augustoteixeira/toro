# Agent Instructions

For the server side, we take inspiration at orbitask at https://github.com/augustoteixeira/orbitask

## Core Principles

- Make **atomic changes only**. One small, focused change at a time.
- Do **not** go beyond what was explicitly requested. If a task says "add X", add X and nothing else.
- Do **not** refactor, rename, reorganize, or "improve" anything that was not part of the request.
- Do **not** add dependencies, files, or boilerplate unless explicitly asked.
- Ask for clarification before making any assumption that would affect the implementation.

## Project Overview

This is a DIY weather station project with two components:

1. **ESP32 firmware** — written in Rust, using `esp-idf` with FreeRTOS for async support and verified TLS. See [`esp/AGENT.md`](esp/AGENT.md).
2. **Web server** — written in Rust using the Rocket framework, serving the web frontend. See [`server/AGENT.md`](server/AGENT.md).

## Development Approach

- Progress is **intentionally slow and deliberate**.
- Each change must be justified by an explicit instruction.
- Prefer correctness and clarity over cleverness or completeness.
- Do not anticipate future steps or scaffold ahead.

## Development Workflow

For each item in the relevant TODO file:

1. **Pick** — the user selects the next TODO item to tackle.
2. **Discuss** — the agent proposes an approach; the user reviews and adjusts.
3. **Agree** — both sides confirm the strategy before any code is written.
4. **Implement** — the agent writes the code.
5. **Review** — verify the change builds, passes tests, or runs correctly.
6. **Commit** — create a focused commit for the completed item.

## Working Dynamics

- The user drives all decisions; agents propose and wait for confirmation.
- When a task requires interactive commands (e.g. `cargo generate`), instruct the user to run them and wait for the result before proceeding.
- Diagnose problems by actually running commands and reading output — do not guess.
