---
name: Feature Specification (Spec Kit)
about: Dual-Mode Plugin Architecture (Library & Standalone CLI Binary) and Pipeline Execution Modes
title: 'Feature: Dual-Mode Plugin Architecture and Pipeline Execution Models'
labels: 'feature, spec-kit, wasm-components'
assignees: ''
---

## Feature Intent

Enable all WASM Geo Host plugins to operate in **Dual-Mode**:
1. **In-Process Reactor Library (`cdylib`/`rlib`)**: Executed directly within `flagme-host` via Wasmtime embedding.
2. **Standalone WASI CLI Binary (`wasi:cli/run`)**: Executable directly via `wasmtime run plugin.wasm` reading from `stdin` and writing to `stdout`.

Furthermore, provide full documentation and execution examples for three advanced pipeline execution models:
- **2a. Unix CLI Streaming Pipeline**: Chaining individual WASM binaries via standard Unix pipes (`|`).
- **2b. WebAssembly Component Composition**: Statically composing plugins into a single unified component via `wac compose` / `wasm-tools compose`.
- **2c. Host Streaming Supervisor**: Orchestrating sandboxed subprocess streams within `flagme-host`.

## Requirements (EARS Syntax)

1. **Ubiquitous:** Every plugin crate shall provide both a library target (`[lib]`) and a standalone executable binary target (`[[bin]]`).
2. **Event-driven:** When invoked via CLI / `wasmtime run`, the plugin shall parse `UnifiedDocument` JSON from `stdin` (or an image file argument if initializing), perform its transformation, and write the enriched `UnifiedDocument` JSON to `stdout`.
3. **Event-driven:** When invoked within `flagme-host`, the plugin shall provide the in-process `Plugin` trait implementation.
4. **State-driven:** While processing inputs from `stdin`, diagnostic and tracing logs shall be written exclusively to `stderr` to prevent JSON corruption on `stdout`.
5. **Unwanted behavior:** If input JSON parsing or file reading fails, the plugin binary shall emit a structured error to `stderr` and exit with a non-zero exit code.

## Scope

- [x] `plugins/*` (`metadata`, `reversegeo`, `countryflag`, `thumbnail`, `ai-tagging`, `duplicate`, `timeline`)
- [x] `crates/host` (`flagme-host`)
- [x] `docs/pipeline_unix_pipes.md` (Mode 2a documentation & invocation examples)
- [x] `docs/pipeline_component_composition.md` (Mode 2b documentation & invocation examples)
- [x] `docs/pipeline_host_supervisor.md` (Mode 2c documentation & invocation examples)

## SpecDD Architecture Alignment (Drift Control)

- [x] **Architecture:** Layering is maintained; plugins depend strictly on `flagme-sdk`.
- [x] **Component Model:** Binary targets compile natively to `wasm32-wasip2` implementing `wasi:cli/run`.
- [x] **Security:** Plugins operate without ambient authority; filesystem access requires explicit CLI directory mappings (`--dir`).
- [x] **Data Model:** Inter-plugin communication strictly uses `UnifiedDocument`.
- [x] **Language & Quality:** 100% Rust with comprehensive test verification.

## Acceptance Criteria

- [x] AC-1 All 7 plugins contain a `src/main.rs` binary entry point supporting stdin/stdout JSON streaming.
- [x] AC-2 All plugins compile cleanly for `wasm32-wasip2` both as cdylib and bin.
- [x] AC-3 `docs/pipeline_unix_pipes.md` describes the Unix CLI streaming pipeline with concrete wasmtime call examples.
- [x] AC-4 `docs/pipeline_component_composition.md` describes component composition with `wac` / `wasm-tools`.
- [x] AC-5 `docs/pipeline_host_supervisor.md` describes host subprocess streaming orchestration.
- [x] AC-6 Test suite passes (`cargo test --all`) and SDD lint reports 0 findings.
