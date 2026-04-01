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
