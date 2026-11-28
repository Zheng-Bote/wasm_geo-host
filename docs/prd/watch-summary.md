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

# PRD — watch meldet den ersten Scan, Duplikate und kennt `--once` (`watch-summary`)

## Problem

`flagme-host watch` wirkt nach dem ersten Scan wie aufgehängt (Issue #71). Es läuft weiter, gibt dem Nutzer aber keinen Hinweis darauf.

Ein echter Lauf am 2026-11-27 (v0.3.0) mit 44 Dateien im Quellordner:

| Dateien                                          | Was der Watcher tat                          | sichtbar beim Standard `RUST_LOG=warn`? |
| :----------------------------------------------- | :------------------------------------------- | :-------------------------------------- |
| 1 neues Bild                                     | verarbeitet und in den Zielordner verschoben | ja (`[watcher] Processed …`)            |
| 21 Bilder schon in `media.db` (gleicher SHA-256) | übersprungen, im Quellordner gelassen        | **nein**, nur `info`                    |
| 22 AppleDouble-Dateien (`._*`)                   | ignoriert                                    | **nein**, gar keine Meldung             |

Danach kam keine Ausgabe mehr. Der Prozess wartete auf neue Dateien, ohne das zu sagen.

- Duplikate bleiben ohne Hinweis im Quellordner liegen und werden nicht einsortiert.
- Ignorierte Dateien werden nicht gezählt.
- Es gibt keinen Weg, den Bestand einmal zu verarbeiten und dann zu beenden (Batch-Import).

## Ziel

Nach dem ersten Scan sieht der Nutzer in einer Zeile, was mit jeder Datei geschah, und dass der Watcher jetzt auf neue Dateien wartet. Jedes übersprungene Duplikat ist sichtbar. Mit `--once` verarbeitet `watch` den Bestand, gibt die Zusammenfassung aus und endet mit einem passenden Exit-Code.

## Nutzer

- **Fotografen und Betreiber**, die einen Ordner importieren oder dauerhaft beobachten lassen.
- **Skripte und Cron-Jobs**, die einen Batch-Import mit `--once` starten und den Exit-Code auswerten.

## Umfang

- **Ergebnis je Datei:** `process_single_file` liefert statt `Option<UnifiedDocument>` ein Ergebnis mit diesen Fällen:
  - **verarbeitet**, mit Dokument;
  - **schon vorhanden**, mit der ID des vorhandenen Dokuments (Suche über den SHA-256 in `Storage`);
  - **gleicher Inhalt wird gerade verarbeitet** (ein zweiter Worker hat dieselbe Datei in Arbeit);
  - **nicht mehr vorhanden oder leer**, weil die Datei zwischen Scan und Verarbeitung verschwunden oder 0 Byte groß ist.

  Fehler (Pipeline, Einsortieren) bleiben `Err`.

- **Ausgabe je Datei auf stdout**, im Stil der bestehenden `Processed`-Zeile:
  - `[watcher] Processed '<name>' -> id=<id> (<city|no-gps>)` wie bisher;
  - `[watcher] Skipped '<name>': already ingested as <id> (not organized)`;
  - `[watcher] Skipped '<name>': same content is being processed`;
  - `[watcher] Failed '<name>': <error>`.

  Die `tracing`-Meldungen auf stderr bleiben, also auch das bisherige `error` bei Fehlern.

- **Erster Scan:**
  - Er sammelt alle Dateien unter dem Quellordner ohne den Zielordner.
  - Er zählt **ignorierte** Dateien: versteckte, AppleDouble (`._*`), abgeleitete (`*.redacted.jpg`, `*.working.jpg`), Dateien unter `cache/` und nicht unterstützte Endungen. Mit `RUST_LOG=info` nennt er jede ignorierte Datei.
  - Dann gibt er alle unterstützten Dateien an die Worker.
- **Zusammenfassung:**
  - Sobald alle Dateien des ersten Scans **verarbeitet** sind, nicht erst eingereiht, gibt `watch` genau eine Zeile auf stdout aus:

    ```
    [watcher] Initial scan of "<dir>": <n> files: <p> processed, <d> already ingested (skipped), <i> ignored (hidden/unsupported), <f> failed; watching for new files (Ctrl+C to stop)
    ```

  - Mit `--once` endet die Zeile auf `done` statt auf `watching …`.
  - `<n>` ist die Summe der anderen Zahlen. „already ingested“ umfasst auch „gleicher Inhalt wird gerade verarbeitet“ und „nicht mehr vorhanden oder leer“. Für jede dieser Dateien steht eine eigene Zeile da.

- **`--once`:**
  - Es wird kein Dateisystem-Beobachter gestartet; verarbeitet wird nur der Bestand.
  - Danach kommt die Zusammenfassung, und der Prozess endet mit Code **0**, wenn keine Datei fehlschlug, sonst mit **1**.
  - Ein leerer oder fehlender Quellordner ergibt eine Zusammenfassung mit 0 Dateien und Code 0. Ein fehlender Ordner kommt zusätzlich mit einer Warnung.
- **Ohne `--once`** bleibt das Verhalten für neue Dateien wie bisher (directory-watcher AC-1 bis AC-4).
- **Kommandozeile:** `--once` kommt in die Usage-Zeile, ins README (Abschnitt 6) und in `docs/deployment-guide.md`, dazu die Zusammenfassung, das Verhalten bei Duplikaten und die Exit-Codes.
- **Nebenbei:** Das README verweist auf `docs/design/image-convert.md`, das seit 0.5.0 unter `docs/archive/design/` liegt. Der Link wird korrigiert.

## Nicht-Ziele

- **Keine Behandlung von Duplikaten:** Sie werden weder verschoben, gelöscht noch umbenannt; nur sichtbar gemacht.
- **Keine Zusammenfassung für später hinzukommende Dateien:** Für sie gibt es weiter eine Zeile je Datei.
- **Kein Fortschrittsbalken und keine maschinenlesbare Ausgabe (JSON):** Skripte nutzen den Exit-Code.
- **Keine Wiederholung fehlgeschlagener Dateien.**
- **Keine Änderung an `process` und `stream`.**

## Akzeptanzkriterien

- **AC-1** Nach dem ersten Scan eines Ordners mit bekanntem Inhalt (neue Bilder, schon vorhandene Bilder, `._*`- und versteckte Dateien, eine nicht unterstützte Endung) steht genau eine Zusammenfassung auf stdout. Ihre Zahlen für gefunden, verarbeitet, schon vorhanden, ignoriert und fehlgeschlagen stimmen mit dem Inhalt überein. Ohne `--once` endet sie mit dem Hinweis, dass der Watcher weiterläuft, und sie erscheint erst, nachdem alle Dateien verarbeitet sind.
- **AC-2** Eine Datei, deren SHA-256 schon in der Datenbank steht, ergibt beim Standard-Log-Level die Zeile `[watcher] Skipped '<name>': already ingested as <id> (not organized)` mit der ID des vorhandenen Dokuments. Die Datei bleibt, wo sie ist.
- **AC-3** `._*`-Dateien, versteckte Dateien, abgeleitete Dateien (`*.redacted.jpg`, `*.working.jpg`) und nicht unterstützte Endungen werden als ignoriert gezählt und nicht verarbeitet. Mit `RUST_LOG=info` wird jede einzeln genannt.
- **AC-4** `watch <DIR> --once` verarbeitet den Bestand, gibt die Zusammenfassung mit `done` aus und endet mit Code 0. Mit einer Datei, deren Verarbeitung scheitert, endet es mit Code 1, die Zusammenfassung zählt sie als `failed`, und eine Zeile `[watcher] Failed '<name>': …` nennt sie. Ein leerer Ordner ergibt 0 Dateien und Code 0.
- **AC-5** Ohne `--once` werden Dateien, die nach dem ersten Scan entstehen, weiter verarbeitet (bisheriges Verhalten von directory-watcher).
- **AC-6** Die Usage-Zeile, das README und `docs/deployment-guide.md` beschreiben `--once`, die Zusammenfassung, die Exit-Codes und was mit Duplikaten geschieht. Das README verlinkt die archivierte Spec von `image-convert` richtig.
