# ADR-001: Rust Core

## Status
Accepted

## Context
Talkcer membutuhkan native audio capture, streaming pipeline, CLI, dan desktop integration.

## Decision
Gunakan Rust untuk core, server, CLI, dan native audio integration.

## Consequences
- Single binary dan kontrol resource yang baik.
- Kompleksitas async dan ownership perlu dikelola.
- Audio capture tetap memerlukan adapter per platform.
