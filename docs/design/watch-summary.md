---
feature: watch-summary
status: accepted
archive-after:
touches:
  - crates/host/src/watcher.rs
  - crates/host/src/main.rs
  - crates/host/src/storage.rs
  - crates/host/src/lib.rs
  - crates/host/tests/**
  - README.md
  - CHANGELOG.md
  - docs/deployment-guide.md
  - docs/arc42.md
---

# Tech Spec — watch meldet den ersten Scan, Duplikate und kennt `--once` (`watch-summary`)

Setzt das PRD `docs/prd/watch-summary.md` um (Issue #71). Die Verfassung hat Vorrang; diese Tech Spec darf sie präzisieren, nie lockern. Es gibt keine neue Regel und deshalb kein ADR.

## Architektur

```
run()
 ├─ scan_existing_files() ─▶ ScanResult { files, ignored }   (synchron, vor den Workern)
 ├─ [ohne --once] notify-Beobachter ─▶ Job { path, initial: false }
 ├─ Jobs des Scans ─▶ Job { path, initial: true }
 └─ Worker (n) ─▶ process_single_file() ─▶ IngestOutcome | Err
                    └─ report(): Zeile auf stdout; bei initial: Tally zählen,
                       nach der letzten Datei des Scans genau einmal die Zusammenfassung
```

**`crates/host/src/watcher.rs`**

- **`IngestOutcome`** (öffentlich):

  ```rust
  pub enum IngestOutcome {
      Processed(Box<UnifiedDocument>),
      AlreadyIngested { id: String },
      InProgress,   // gleicher SHA-256 gerade bei einem anderen Worker
      Gone,         // zwischen Scan und Verarbeitung verschwunden oder 0 Byte
  }
  ```

  `process_single_file(...) -> anyhow::Result<IngestOutcome>`. Die bisherigen `Ok(None)`-Fälle werden je nach Ursache zu `Gone`, `InProgress` oder `AlreadyIngested { id }`. Die ID kommt aus `Storage::document_id_for_sha256`. Fehler in Pipeline und Einsortieren bleiben `Err`.

- **`WatcherConfig.once: bool`** (Standard `false`).
- **`scan_existing_files(&self) -> ScanResult`** ist synchron und rekursiv:
  - es überspringt den Zielordner;
  - es teilt die Dateien mit `is_supported_media_file` in `files` und `ignored`;
  - für jede ignorierte Datei schreibt es `tracing::info!(path, "Ignoring file (hidden, derived or unsupported)")`;
  - fehlt der Quellordner, schreibt es `tracing::warn!` und liefert ein leeres Ergebnis.
- **`Job { path: PathBuf, initial: bool }`** ersetzt `PathBuf` im Kanal.
- **`ScanTally`** (`Arc<Mutex<…>>`) zählt `processed`, `already`, `failed` und die noch offenen Dateien `remaining` des ersten Scans; `found` und `ignored` stehen von Anfang an fest. Ein Worker zählt nach jeder Datei mit `initial: true`. Fällt `remaining` dabei auf 0, gibt er die Zusammenfassung aus und setzt einen Merker, damit sie genau einmal erscheint. Hat der Scan keine unterstützte Datei, gibt `run` die Zusammenfassung direkt nach dem Scan aus.
- **Zeilen je Datei** (`println!`, stdout):

  | Ergebnis          | Zeile                                                                    |
  | :---------------- | :----------------------------------------------------------------------- |
  | `Processed`       | `[watcher] Processed '<name>' -> id=<id> (<city\|no-gps>)` (unverändert) |
  | `AlreadyIngested` | `[watcher] Skipped '<name>': already ingested as <id> (not organized)`   |
  | `InProgress`      | `[watcher] Skipped '<name>': same content is being processed`            |
  | `Gone`            | `[watcher] Skipped '<name>': file vanished or is empty`                  |
  | `Err(e)`          | `[watcher] Failed '<name>': <e>` und wie bisher `tracing::error!`        |

- **Zusammenfassung:**

  ```text
  [watcher] Initial scan of "<dir>": <found> files: <processed> processed, <already> already ingested (skipped), <ignored> ignored (hidden/unsupported), <failed> failed; watching for new files (Ctrl+C to stop)
  ```

  Mit `once` endet die Zeile auf `; done`. `<found>` ist `files.len() + ignored.len()`, `<already>` umfasst `AlreadyIngested`, `InProgress` und `Gone`.

- **`run(&self) -> anyhow::Result<WatchReport>`** mit `WatchReport { found, processed, already, ignored, failed }`:
  - **ohne `once`:** wie bisher. Die notify-Jobs tragen `initial: false`; `run` kehrt nach `stop()` zurück und liefert die Zahlen des ersten Scans;
  - **mit `once`:** kein `RecommendedWatcher`. Die Jobs des Scans werden eingereiht, dann wird der Sender geschlossen. Die Worker enden, wenn der Kanal leer und geschlossen ist; `run` wartet auf sie und liefert den Bericht.
- `is_supported_media_file` und `detect_mime_type` bleiben unverändert.

**`crates/host/src/storage.rs`**

- `pub fn document_id_for_sha256(&self, sha256: &str) -> Option<String>`:
  - zuerst im Speicher (`photos`: ID → `FileInfo`);
  - sonst `sqlite3` mit `SELECT id FROM photos WHERE sha256 = '<escaped>' LIMIT 1;`.
- `has_sha256` wird zu `document_id_for_sha256(..).is_some()`.

**`crates/host/src/main.rs`**

- **Option `--once`:** `let once = args.iter().any(|a| a == "--once");`.
- **Usage:** `Usage: flagme-host watch <DIR> [--target-dir <DIR>] [--config <CONFIG>] [--workers <N>] [--organize-by <PATTERN>] [--copy] [--once]`.
- **Startzeile:** Ohne `--once` bleibt sie. Mit `--once` lautet sie `Importing "<DIR>" once (workers: <n>)`.
- **Exit-Code:** Nach `run` mit `once`: `std::process::exit(1)`, wenn `report.failed > 0`, sonst normales Ende mit 0.

**`crates/host/src/lib.rs`:** zusätzlich `IngestOutcome` und `WatchReport` exportieren.

## Schnittstellen

- **CLI:** `flagme-host watch <DIR> … [--once]`.
- **Exit-Codes:**
  - mit `--once`: 0 ohne Fehler, 1 bei mindestens einer fehlgeschlagenen Datei;
  - ohne `--once` wie bisher (Ende nur durch Signal oder Fehler beim Start).
- **Ausgabekanäle:** Zeilen je Datei und die Zusammenfassung gehen auf stdout, Log-Meldungen (`tracing`) auf stderr. Das dokumentieren README und Deployment-Guide.
- **Rust-API:**
  - `process_single_file` liefert `anyhow::Result<IngestOutcome>`;
  - `DirectoryWatcher::run` liefert `anyhow::Result<WatchReport>`;
  - `WatcherConfig` hat das neue Feld `once`;
  - `Storage::document_id_for_sha256`.

## Datenmodell

Keine Änderung am Dokumentmodell, an WIT oder an der Datenbank.

## Umsetzung der Akzeptanzkriterien

Tests in `crates/host/tests/watch_summary_test.rs`. Sie starten `flagme-host` als Prozess (`CARGO_BIN_EXE_flagme-host`) mit einer Testkonfiguration:

- Datenbank im Testordner, `plugin_dir` auf die gebauten Komponenten;
- Pipeline `metadata`, `timeline`. Das hält die Tests schnell und erfüllt ADR 0012, weil `privacy-redact` nicht in der Pipeline steht.

Die Bilder entstehen mit `common::plain_jpeg` in verschiedenen Größen, also mit verschiedenem SHA-256. Ohne `--once` liest ein Hilfsthread stdout zeilenweise; der Test wartet mit Zeitgrenze auf die erwartete Zeile und beendet den Prozess dann.

| AC   | Umsetzung                                   | Test                                                                                                                                                                                                                                                                                                                                                   |
| :--- | :------------------------------------------ | :----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| AC-1 | `ScanTally`, Zusammenfassung                | `test_ac1_initial_scan_summary`: 2 neue Bilder, 1 schon vorhandenes (vorher mit `--once` importiert), `._a.jpg`, `.hidden.jpg`, `notes.txt`. Die Zusammenfassung lautet `6 files: 2 processed, 1 already ingested (skipped), 3 ignored (hidden/unsupported), 0 failed; watching …`. Sie erscheint genau einmal und nach den beiden `Processed`-Zeilen. |
| AC-2 | `AlreadyIngested`, `document_id_for_sha256` | `test_ac2_duplicate_is_visible_and_stays`: mit `--target-dir`. Die Skip-Zeile nennt die ID aus dem ersten Lauf, und die Datei liegt danach noch im Quellordner.                                                                                                                                                                                        |
| AC-3 | `ScanResult.ignored`                        | `test_ac3_ignored_files_counted_and_listed_at_info`: Datei unter `cache/`, `x.redacted.jpg`, `y.working.jpg`, `._z.jpg`, `.h.jpg` und `notes.txt` werden nicht verarbeitet (keine Zeile `Processed`, nichts in der Datenbank). Mit `RUST_LOG=info` nennt stderr jede von ihnen.                                                                        |
| AC-4 | `once`, Exit-Code                           | `test_ac4_once_exit_codes`: Bestand ergibt Code 0 und `; done`. Ein schreibgeschützter Zielordner (`--target-dir`, Modus 0o555) ergibt `Failed`-Zeile, `1 failed` und Code 1. Ein leerer Ordner ergibt `0 files` und Code 0.                                                                                                                           |
| AC-5 | notify-Jobs                                 | `test_ac5_new_files_after_scan_are_processed`: nach der Zusammenfassung ein neues Bild schreiben und auf seine `Processed`-Zeile warten.                                                                                                                                                                                                               |
| AC-6 | Dokumentation                               | `test_ac6_usage_and_docs`: Usage-Text, README-Abschnitt 6 und Deployment-Guide nennen `--once`, die Zusammenfassung, die Exit-Codes und Duplikate. Der README-Link auf `docs/archive/design/image-convert.md` existiert.                                                                                                                               |

**Anpassungen bestehender Tests** (die Kennungen bleiben):

- `watcher_test.rs` (directory-watcher AC-1 bis AC-4):
  - `WatcherConfig { …, once: false }`;
  - `process_single_file` wird auf `IngestOutcome` geprüft: AC-3 erwartet beim zweiten Lauf `AlreadyIngested { id }` mit der ID des ersten, statt `None`.

## Sicherheit

- Keine neue Capability, keine Änderung an Sandbox oder Grants.
- Die neue Abfrage `document_id_for_sha256` nutzt dasselbe Escaping (`escape_sql`) wie `has_sha256`; der SHA-256 ist ohnehin hexadezimal.
- Ausgegeben werden nur Dateinamen und IDs, keine Dateiinhalte und keine Metadaten.

## Offene Punkte

- Keine.
