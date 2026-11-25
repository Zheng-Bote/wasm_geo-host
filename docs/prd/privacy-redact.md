---
feature: privacy-redact
status: draft
archive-after:
touches:
  - "crates/models/**"
  - "plugins/privacy-redact/**"
  - "crates/host/**"
  - "crates/host/migrations/**"
  - "Cargo.toml"
---

# PRD — privacy-redact (`privacy-redact`)

## Problem

Öffentlich publizierte Medien (Websites, Blogs, Galerien) enthalten häufig sensible personenbezogene Daten, die dem Datenschutz (z. B. DSGVO/GDPR) unterliegen oder Sicherheitsrisiken bergen: Gesichter unbeteiligter Passanten, Autokennzeichen, metergenaue GPS-Koordinaten privater Wohnorte sowie Hardware-Seriennummern von Kamera und Objektiven.

## Ziel

Bereitstellung des Plugins `flagme-plugin-privacy-redact` zur automatisierten Bild-Anonymisierung und Metadaten-Bereinigung. Das Plugin unterstützt optische Verpixelung (Pixelate), Weichzeichnen (Blur) und Schwärzung (Blackout) über erkannte Bounding-Boxen (z. B. aus `doc.faces`), GPS-Fuzzing (Reduktion der Dezimalstellen auf Stadtteil-Ebene) und die Entfernung von Hardware-Seriennummern aus den Metadaten. Die Ergebnisse werden in `PrivacyInfo` und in SQLite festgehalten.

## Nutzer

- Fotografen, Verlage und Plattformbetreiber, die Bildinhalte vor dem Export oder der Veröffentlichung DSGVO-konform anonymisieren müssen.

## Umfang

Die Umsetzung umfasst:
- Datenstruktur `PrivacyInfo` in `wasm-geo-models` mit Serde-Serialisierung.
- Plugin-Crate `plugins/privacy-redact` mit CDYLib & WASI-Binärsupport (`wasm32-wasip2`).
- Optische Redaktions-Algorithmen (Pixelate, Blur, Blackout) für Bounding-Boxen.
- Metadaten-Sanitisierung: GPS-Fuzzing (z. B. Rundung auf 2 Dezimalstellen ~1 km) und Säuberung von Seriennummern (`CameraSerialNumber`, `LensSerialNumber`).
- SQLite-Persistierung von Anonymisierungs-Status und Exportpfaden.
- Host-Pipeline-Registrierung in `flagme-host`.
- Integrationstests je Kriterium mit Kennzeichnung.

## Nicht-Ziele

- Keine Wiederherstellung originaler Pixel nach destruktivem Blackout (gewollte irreversible Sicherheit).

## Akzeptanzkriterien

- **AC-1** Model Extension: `wasm-geo-models` stellt `PrivacyInfo` bereit und serialisiert diese in `UnifiedDocument.privacy`.
- **AC-2** Plugin Compilation: `flagme-plugin-privacy-redact` kompiliert als Plugin-Crate mit `cdylib` und `[[bin]]` unter `wasm32-wasip2`.
- **AC-3** Visual Obfuscation: Das Plugin wendet Verpixelung (Pixelate) und Schwärzung (Blackout) destruktiv auf erkannte Bounding-Boxen an.
- **AC-4** GPS Coordinate Fuzzing: GPS-Koordinaten werden auf konfigurierte Dezimalstellen gerundet, wenn `fuzz_gps_coordinates` aktiviert ist.
- **AC-5** Hardware Serial Purging: Hardware-Seriennummern werden aus `raw_tags` entfernt, wenn `strip_serial_numbers` aktiviert ist.
- **AC-6** Integration Verification: Integrationstests weisen die korrekte Anonymisierung von Bildbereichen und Metadaten nach.
