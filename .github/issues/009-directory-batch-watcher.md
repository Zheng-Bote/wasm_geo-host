# Feature Spec: Directory Watcher & Batch Processing (`flagme-host watch`)

- **Feature Name**: `directory-batch-watcher`
- **Component**: `crates/host`
- **Issue Reference**: `data/ideen.md` Punkt 2.3
- **Issue Type**: Platform Feature / Ingest Daemon

---

## 1. Problem Statement & Motivation
Currently, `flagme-host` processes single files via manual CLI invocations (`flagme-host process <FILE>`). In production use cases (home servers, NAS, photography workflows), new media files continuously arrive in an intake directory (e.g. SD card mount, camera upload, Dropbox sync).

A dedicated `flagme-host watch` daemon watches directories in real-time, queues files through the plugin pipeline concurrently, and organizes processed files into structured directory trees.

---

## 2. Technical Specification

### 2.1 CLI Subcommand
```bash
flagme-host watch /media/import/ \
  --config config.yaml \
  --target-dir /media/library/ \
  --organize-by "{year}/{month}/{country}/{city}"
```

### 2.2 Ingest Architecture
- **Filesystem Event Listener**: Uses the cross-platform `notify` crate to watch incoming files (handling `Create` and `CloseWrite` events).
- **Concurrency & Backpressure**: Tokio worker pool (`tokio::sync::mpsc`) processing up to $N$ files in parallel (configurable via `config.yaml`).
- **Idempotency & Deduplication**: Checks `sha256` against `media.db` before executing heavy stages.
- **Automated Media Filing**: Moves or symlinks processed files into templated paths based on resolved metadata (e.g. `2026/11/Germany/Berlin/IMG_0001.jpg`).

---

## 3. Acceptance Criteria
- [ ] **AC-1**: `flagme-host watch <DIR>` watches filesystem directories without busy polling.
- [ ] **AC-2**: Thread pool concurrently executes the configured plugin pipeline on detected media files.
- [ ] **AC-3**: Duplicate detection skips re-processing already ingested files.
- [ ] **AC-4**: Target organization moves/copies files into customizable directory hierarchies based on resolved metadata.
