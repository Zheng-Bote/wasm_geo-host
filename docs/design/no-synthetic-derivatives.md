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

# Tech Spec — Keine erfundenen Hashes, Wetterdaten und Thumbnails (`no-synthetic-derivatives`)

Setzt das PRD `docs/prd/no-synthetic-derivatives.md` um. Die Verfassung hat Vorrang; diese Tech Spec darf sie präzisieren, nie lockern. Grundlage ist [ADR 0002](../adr/0002-plugins-without-engine-no-output.md).

## Architektur

Alle drei Plugins lesen ausschließlich über `ctx.capabilities.fs_read` und schreiben ausschließlich über `fs_write`. Bildverarbeitung läuft in reinem Rust mit der Crate `image` (`default-features = false`, Features `jpeg`, `png`, `webp`, `tiff`, `gif`); sie baut auch für `wasm32-wasip2`.

## Schnittstellen

### `duplicate`

```text
fs_read ─ Err ─► warn "duplicate: cannot read '<path>': …; no hashes"; doc unverändert
        └ Ok(bytes) ─► sha256 = SHA-256(bytes)
                     └ phash  = dhash(bytes)  // None + info, wenn nicht dekodierbar
```

- `pub fn compute_dhash(bytes: &[u8]) -> Option<String>`: dekodieren (`image::load_from_memory`), EXIF-Orientierung anwenden, Graustufen, `resize_exact(9, 8, Triangle)`, Bit gesetzt, wenn linkes Pixel heller als rechtes; 16 Hex-Zeichen.
- Ohne `phash` entfällt der pHash-Abgleich (exakt und per Index); der SHA-256-Abgleich bleibt.
- Entfernt: `compute_phash` über komprimierte Bytes, `find_image_entropy_payload`, der Pfad-Fallback.

### `weather-enrich`

- Reihenfolge: Koordinaten → Aufnahmezeit (sonst warn `weather-enrich: no capture time for '<path>'; weather not queried`) → KV-Cache → HTTP.
- HTTP-Fehler oder unlesbare Antwort: warn `weather-enrich: no measured weather for '<path>': …`, kein Cache-Eintrag.
- Entfernt: `compute_climatological_fallback`, Ersatzdatum `2025-07-15`, Feld `WeatherSettings::offline_fallback`; `offline_fallback` wird über `SettingsReader::not_supported` gemeldet.

### `thumbnail`

- Quelle über `fs_read`, Format über den Inhalt (`ImageReader::with_guessed_format`), EXIF-Orientierung über `ImageDecoder::orientation` und `DynamicImage::apply_orientation`.
- Je Zielgröße: ist die längere Seite größer als die Zielgröße, `resize(size, size, Lanczos3)` (Seitenverhältnis bleibt), sonst Originalgröße; RGB8; JPEG mit `quality` (Standard: Encoder-Standard 75); `fs_write` nach `<output_dir>/<cache_key>_<size>.jpg`.
- Eintrag mit den tatsächlichen Maßen nur nach erfolgreichem Schreiben; sonst warn.
- Entfernt: `FALLBACK_JPEG_1X1`, Kopie des Originals, `sips`, `std::fs`, Dateiendungs-Prüfung (`.dat`-Sonderfall).

## Datenmodell

```rust
pub struct DuplicateInfo {
    pub sha256: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub phash: Option<String>,   // None: Bild nicht dekodierbar
    pub duplicates: Vec<String>,
}
```

Ältere Dokumente mit `phash` als String bleiben lesbar (`Some`). In der Tabelle `duplicates` (Spalte `phash TEXT NOT NULL`) wird `None` als leere Zeichenkette gespeichert; das vollständige Dokument liegt weiterhin als JSON in `documents_store`. Die Web-UI zeigt fehlendes `phash` als `N/A`.

## Umsetzung der Akzeptanzkriterien

| AC | Umsetzung | Test (`crates/host/tests/derivatives_test.rs`) |
|---|---|---|
| AC-1 | `duplicate`: kein Pfad-Fallback | `test_ac1_duplicate_unreadable_file` |
| AC-2 | `compute_dhash`, `Option<String>` | `test_ac2_sha256_and_dhash`, `test_ac2_undecodable_image_has_no_phash` |
| AC-3 | `weather-enrich` ohne Fallback/Ersatzdatum | `test_ac3_weather_without_measurement`, `test_ac3_weather_without_capture_time` |
| AC-4 | `not_supported("offline_fallback")` | `test_ac4_offline_fallback_not_supported` |
| AC-5 | `thumbnail` mit `image` | `test_ac5_thumbnails_are_real_files` |
| AC-6 | `thumbnail` Fehlerpfade | `test_ac6_thumbnail_failures` |

## Sicherheit

Entfernt den Prozessaufruf `sips` und direkte `std::fs`-Zugriffe in `thumbnail` (Verfassung Security 1–2, Teil von #25).

## Offene Punkte

- HEIC/AVIF-Unterstützung (benötigt einen Decoder außerhalb von reinem Rust oder eine echte Engine; eigenes ADR).
