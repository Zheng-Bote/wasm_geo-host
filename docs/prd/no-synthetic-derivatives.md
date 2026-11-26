---
feature: no-synthetic-derivatives
status: draft
archive-after:
touches:
  - plugins/duplicate/**
  - plugins/weather-enrich/**
  - plugins/thumbnail/**
  - crates/models/**
  - crates/host/src/**
  - crates/host/ui/**
  - crates/host/tests/**
  - config*.yaml
  - Cargo.lock
  - README.md
  - CHANGELOG.md
  - docs/pipeline_host_supervisor.md
---

# PRD — Keine erfundenen Hashes, Wetterdaten und Thumbnails (`no-synthetic-derivatives`)

## Problem

Ein echter Lauf von `flagme-host stream` (PR #39) zeigt, dass drei weitere Plugins erfundene Werte als echte melden (Issue #40). Das verstößt gegen die Verfassung (*Strikte Regeln* 2) und gegen [ADR 0002](../adr/0002-plugins-without-engine-no-output.md), Regel 1:

- `duplicate` hasht bei nicht lesbarer Datei den **Pfad-String** und speichert das Ergebnis als Datei-SHA256. Der „perceptual hash“ wird über die komprimierten Bytes statt über Bildpixel berechnet.
- `weather-enrich` erfindet ohne HTTP Wetterdaten aus einem „klimatologischen Modell“ (standardmäßig aktiv), legt sie im KV-Cache ab und fragt ohne Aufnahmezeit ein festes Ersatzdatum ab.
- `thumbnail` meldet ungemessene Maße (immer `Zielgröße × Zielgröße`), schreibt als Ersatz ein 1×1-JPEG oder eine Kopie des Originals und listet Einträge für Dateien, die nicht existieren. Es nutzt `sips` und `std::fs` an der Capability-Prüfung vorbei.

## Ziel

Hashes, Wetterdaten und Thumbnails sind entweder aus der Quelle bestimmt oder fehlen nachvollziehbar mit einer Warnung.

## Nutzer

- Betreiber der Pipeline, die Duplikaterkennung, Wetteranreicherung und die Thumbnails der Web-UI nutzen.

## Umfang

- `duplicate`: Hashes nur aus gelesenen Dateibytes; echter dHash über dekodierte Pixel; `phash` ist optional.
- `weather-enrich`: Klima-Fallback und Ersatzdatum entfernen; Setting `offline_fallback` wird ignoriert.
- `thumbnail`: Skalierung mit der Crate `image` (Seitenverhältnis, EXIF-Orientierung, kein Hochskalieren); nur geschriebene Dateien werden gelistet; kein Ersatzbild, kein Prozessaufruf, kein `std::fs`.
- Anpassung von Datenmodell (`DuplicateInfo.phash`), Speicherung, Web-UI, Tests, READMEs und CHANGELOG.

## Nicht-Ziele

- Keine Dekodierung von HEIC/AVIF (in reinem Rust nicht verfügbar); solche Dateien erhalten weder Thumbnails noch `phash`.
- Kein Zwischenspeicher für bereits erzeugte Thumbnails.

## Akzeptanzkriterien

- **AC-1** `duplicate`: Ist die Datei nicht lesbar, bleiben `doc.duplicate` und `doc.file.sha256` unverändert und eine Warnung wird geloggt.
- **AC-2** `duplicate`: `sha256` ist der SHA-256 der Dateibytes. `phash` ist ein 64-Bit-dHash über die dekodierten Pixel (Graustufen, 9×8); dasselbe Motiv in anderer Kodierung (JPEG-Qualität, PNG) liegt innerhalb von 4 Bit Hamming-Abstand, ein anderes Motiv deutlich darüber. Ist das Bild nicht dekodierbar, ist `phash` leer (`None`), `sha256` wird trotzdem gesetzt.
- **AC-3** `weather-enrich`: Ohne gemessene Daten (HTTP-Fehler oder unlesbare Antwort) bleibt `doc.weather` `None`, nichts wird gecacht und eine Warnung wird geloggt. Ohne Aufnahmezeit (`timeline.taken_at`) wird nichts abgefragt, `doc.weather` bleibt `None` und eine Warnung wird geloggt.
- **AC-4** `weather-enrich`: Das Setting `offline_fallback` wird mit der Warnung „not supported“ ignoriert.
- **AC-5** `thumbnail`: Jeder Eintrag in `doc.thumbnails` zeigt auf ein existierendes, dekodierbares JPEG, dessen Maße `width`/`height` entsprechen. Das Seitenverhältnis der (nach EXIF-Orientierung gedrehten) Quelle bleibt erhalten, die längere Seite ist `min(Zielgröße, Originalgröße)`.
- **AC-6** `thumbnail`: Ist die Quelle nicht lesbar oder nicht dekodierbar oder schlägt das Schreiben fehl, entsteht kein Eintrag dafür und eine Warnung wird geloggt; es wird kein Ersatzbild geschrieben.
