---
feature: directory-watcher
status: accepted
archive-after:
touches:
  - 'crates/host/**'
---

# PRD — directory-watcher (`directory-watcher`)

## Problem

Bislang verarbeitet `flagme-host` Mediendateien ausschließlich über manuelle Einzelaufrufe (`flagme-host process <FILE>`). In produktiven Einsatzszenarien (z. B. NAS-Systeme, Home-Server, Kamera-Importordner oder Cloud-Sync) treffen Bild- und Mediendateien fortlaufend in einem Eingangsverzeichnis ein. Ein manueller Aufruf für jede einzelne Datei ist unpraktikabel. Zudem fehlt ein automatisierter Mechanismus, um verarbeitete Dateien anhand ihrer analysierten Metadaten (Aufnahmedatum, Land, Stadt) strukturiert in einer Zielverzeichnis-Hierarchie abzulegen und bereits verarbeitete Dateien ohne erneuten Pipeline-Durchlauf zu erkennen (Deduplizierung).

## Ziel

Bereitstellung eines robusten, ereignisgesteuerten Hintergrund-Daemons (`flagme-host watch`) zur Überwachung von Verzeichnissen und zur nebenläufigen Batch-Verarbeitung eingehender Mediendateien:

1. Rekursive Dateisystemüberwachung ohne Polling mittels des `notify`-Crates.
2. Nebenläufiger Worker-Pool (Tokio Worker / Channel) zur parallelen Verarbeitung neuer Dateien durch die konfigurierte Plugin-Pipeline.
3. Integrierte Duplikaterkennung via SHA-256 Hash vor rechenintensiven Analysestufen.
4. Flexible Organisation und Ablage verarbeiteter Mediendateien in Zielpfade nach konfigurierbarem Template-Muster (z. B. `{year}/{month}/{country}/{city}`).

## Nutzer

- Endanwender, Fotografen und Server-Betreiber, die einen Importordner für neue Fotos überwachen und automatisch analysieren lassen möchten.
- Medienplattformen und Automationsdienste, die eine Batch-Ingestion mit strukturierter Archivierung benötigen.

## Umfang

Die Umsetzung umfasst:

- Dateisystem-Event-Listener mit `notify` zur Beobachtung von Verzeichnissen auf Neuerstellungen und Schreibabschlüsse.
- Nebenläufige Abarbeitungswarteschlange (Tokio Worker Task Pool) mit einstellbarer Parallelität (`workers`).
- Idempotente Duplikatprüfung anhand des SHA-256 Datei-Hashes gegen `media.db` vor Pipeline-Ausführung.
- Dateiorganisationsmodul (`organize_media`), das Dateien nach Metadaten-Templates in Zielverzeichnisse kopiert oder verschiebt.
- CLI-Subcommand `flagme-host watch <DIR> --target-dir <DIR> [--config <CONFIG>] [--workers <N>] [--organize-by <TEMPLATE>] [--copy]`.
- Integrationstests je Kriterium mit Kennzeichnung.

## Nicht-Ziele

- Keine grafische Benutzeroberfläche oder Dashboard (Gegenstand von Issue #10).
- Kein cloud-basiertes Remote-Dateisystem-Polling (z. B. S3 / WebDAV Daemon).

## Akzeptanzkriterien

- **AC-1** Non-blocking Directory Watcher: `flagme-host watch <DIR>` überwacht das Dateisystem ereignisgesteuert ohne Busy-Polling über das `notify`-Crate.
- **AC-2** Concurrent Worker Ingestion: Ein Worker-Pool verarbeitet eingehende Mediendateien nebenläufig über die konfigurierte Plugin-Pipeline.
- **AC-3** Duplicate Ingestion Skip: Bei bereits in der Datenbank vorhandenem SHA-256-Hash wird die erneute Pipeline-Verarbeitung übersprungen.
- **AC-4** Metadata-Driven Target Organization: Verarbeitete Mediendateien werden gemäß konfigurierbarem Template-Muster (z. B. `{year}/{month}/{country}/{city}`) in Zielverzeichnissen strukturiert abgelegt.
- **AC-5** CLI Watch Subcommand Integration: Der Befehl `flagme-host watch <DIR> --target-dir <DIR>` ist vollständig in die CLI integriert und unterstützt Worker-Anzahl sowie Organisationsmuster.
