---
feature: directory-watcher
status: archived
archive-after:
touches:
  - "crates/host/**"
release: 0.2.0
---

# Tech Spec — directory-watcher (`directory-watcher`)

Setzt das PRD `docs/prd/directory-watcher.md` um. Die Verfassung hat Vorrang; diese Tech Spec darf sie präzisieren, nie lockern.

## Architektur & Komponenten

Die Verzeichnisüberwachung und Batch-Ingestion erweitert `flagme-host` um ein ereignisgesteuertes Ingest-Subsystem:

```text
+-------------------------------------------------------------------------+
|                       flagme-host watch <DIR>                           |
+------------------------------------+------------------------------------+
                                     |
                                     v
+-------------------------------------------------------------------------+
|                  DirectoryWatcher (notify Crate)                        |
|  - Ereignisse: Create / Modify / CloseWrite                             |
|  - Rekursive Überwachung ohne Polling (AC-1)                            |
+------------------------------------+------------------------------------+
                                     | Event channel (mpsc)
                                     v
+-------------------------------------------------------------------------+
|                   IngestionManager & Worker Pool                        |
|  - Parallelität N Worker (AC-2)                                         |
|  - SHA-256 Hash-Berechnung & Duplikatsprüfung via Storage (AC-3)        |
|  - Pipeline-Ausführung (Metadata, Geo, Flags, Timeline, etc.)           |
+------------------------------------+------------------------------------+
                                     |
                                     v
+-------------------------------------------------------------------------+
|                   MediaOrganizer (Zielablage)                           |
|  - Template Rendering: {year}/{month}/{country}/{city}/{filename}       |
|  - Erstellung der Zielverzeichnisse & Copy/Move (AC-4)                  |
|  - Persistierung der finalen Metadaten in SQLite (media.db)             |
+------------------------------------+------------------------------------+
```

### 1. Dateisystem-Event-Listener (`notify`) (AC-1)
- Verwendung des `notify`-Crates (empfohlen v6 oder v8/v9).
- Registrierung eines rekursiven Watchers auf das angegebene Quellverzeichnis.
- Filterung relevanter Events: Neuerstellung von Mediendateien (`EventKind::Create(_)`, `EventKind::Modify(_)`, Dateiendungen `.jpg`, `.jpeg`, `.png`, `.webp`, `.tiff`, etc.).
- Erfüllt **AC-1**: Ereignisgesteuerte Dateisystemüberwachung ohne ressourcenintensives Polling.

### 2. Nebenläufiger Worker-Pool & Batch Ingestion (AC-2)
- Erstellung eines Tokio `mpsc::channel` zur Entkopplung von Dateisystemereignissen und Verarbeitung.
- Einstellbare Worker-Anzahl $N$ (Default: Anzahl logischer CPU-Kerne oder CLI `--workers <N>`).
- Jeder Worker entnimmt Pfade aus dem Channel und führt die `Pipeline::process`-Methode aus.
- Erfüllt **AC-2**: Konkurrente Abarbeitung über Worker-Pool.

### 3. Duplikatprüfung & Idempotenz (AC-3)
- Vor Beginn rechenintensiver Analysen berechnet der Ingestion-Worker den SHA-256 Hash der Datei.
- Abfrage via `Storage::has_sha256(&str)` oder `Storage::is_duplicate_hash(&str)`.
- Falls die Datei bereits im Index existiert, wird die Verarbeitung mit Logging übersprungen.
- Erfüllt **AC-3**: Vermeidung doppelter Verarbeitung und redundanter Datenbankspeicherung.

### 4. Metadaten-basierte Zielorganisation (AC-4)
- Auswertung der erzeugten `UnifiedDocument`-Felder:
  - `{year}`: Aufnahmejahr aus `timeline.year` oder DateTime oder aktuelles Jahr (Fallback).
  - `{month}`: Zweistelliger Monat (z. B. `11`).
  - `{country}`: Land aus `location.country` oder Country Code (Fallback `Unknown_Country`).
  - `{city}`: Stadt aus `location.city` (Fallback `Unknown_City`).
  - `{filename}`: Originaler Dateiname.
- `MediaOrganizer::organize(&self, doc: &UnifiedDocument, target_dir: &Path, pattern: &str, copy_only: bool) -> anyhow::Result<PathBuf>`
- Erfüllt **AC-4**: Strukturierte Ablage nach konfiguriertem Muster.

### 5. CLI Watch Subcommand Integration (AC-5)
- Parameter:
  `flagme-host watch <DIR> --target-dir <DIR> [--config <CONFIG>] [--workers <N>] [--organize-by <TEMPLATE>] [--copy]`
- Startet den Daemon, initialisiert DB-Schema, Pipeline und Watcher.
- Erfüllt **AC-5**: Vollwertige CLI-Steuerung.
