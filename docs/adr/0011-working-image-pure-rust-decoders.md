---
status: accepted
date: 2026-11-28
supersedes: []
references:
  - docs/adr/0002-plugins-without-engine-no-output.md
  - docs/adr/0003-wasm-component-execution.md
  - docs/adr/0009-plugin-inference-rten.md
---

# ADR 0011 — Arbeitsbild für Bild-Plugins mit Decodern in reinem Rust

## Kontext

Der Watcher nimmt JPEG, PNG, WebP, TIFF, HEIC und AVIF an (`crates/host/src/watcher.rs`), BMP noch nicht. Die Plugins, die Pixel verarbeiten, lesen aber jeweils selbst `file.path` und können nur einen Teil davon:

| Plugin                                     | dekodiert heute            | EXIF-Drehung |
| :----------------------------------------- | :------------------------- | :----------- |
| `face-detection`, `ocr-text`, `ai-tagging` | JPEG, PNG                  | nein         |
| `privacy-redact`                           | nur JPEG                   | nein         |
| `thumbnail`, `duplicate` (dHash)           | JPEG, PNG, WebP, TIFF, GIF | ja           |

Ein HEIC-Foto vom Smartphone wird deshalb weder auf Gesichter geprüft noch geschwärzt. Das ist eine Datenschutzlücke. Außerdem drehen manche Plugins nach EXIF und andere nicht. Die Gesichtsrahmen beziehen sich deshalb auf ein anderes Bild als die Thumbnails.

Bisher hieß es, HEIC und AVIF ließen sich in reinem Rust nicht dekodieren (PRD `no-synthetic-derivatives`, Nicht-Ziele). Seit Herbst 2026 gibt es solche Decoder.

**Prototyp vom 2026-11-28** (Release-Build, `wasm32-wasip2` mit SIMD, wasmtime ohne Threads, macOS, Apple Silicon). Getestet wurden aus `apollo11.jpg` (gemeinfrei) erzeugte Testbilder und ein echtes HEIC eines Galaxy S23 Ultra. Dieses besteht aus 4 × 8 Kacheln zu 512 px, ist um 270° gedreht (`irot`) und liegt nur lokal vor. Verglichen wurde mit libheif 1.23.7 über ImageMagick.

| Crate (Lizenz, `unsafe`-Fundstellen)                                                    | 1600 × 1257 | Galaxy 1868 × 4000 |  12 MP | 48 MP (Speicher nötig) |             PSNR zu libheif |
| :-------------------------------------------------------------------------------------- | ----------: | -----------------: | -----: | ---------------------: | --------------------------: |
| **`heic-rs` 0.1.5** (MIT/Apache-2.0, `forbid(unsafe_code)`, keine Pflicht-Abhängigkeit) |      113 ms |             304 ms | 523 ms |       2,4 s (≤ 512 MB) |                    48–55 dB |
| `zune-heic` 0.5.0 (MIT/Apache-2.0/Zlib, 63)                                             |      122 ms |             280 ms | 477 ms |       1,7 s (≤ 512 MB) |                    62–72 dB |
| `heic-decoder` 0.1.1 (MIT/Apache-2.0, 1)                                                |      127 ms |             315 ms | 532 ms |         2,0 s (≤ 1 GB) |                    48–55 dB |
| `heifer` 0.9.0 (MIT/Apache-2.0, 1)                                                      |       95 ms |             320 ms | 359 ms |         1,3 s (≤ 1 GB) |                    39–47 dB |
| **`oxideav-avif` 0.0.18** (MIT, 0), 8/10 Bit                                            |  186/199 ms |                  — | 738 ms |  24 MP: 1,4 s (512 MB) |                    58–72 dB |
| `image` 0.25 (WebP, TIFF, BMP, PNG)                                                     |     1–62 ms |                  — |      — |                      — | verlustfrei bzw. wie Quelle |

- Alle Decoder geben das Galaxy-Foto richtig gedreht aus, also mit denselben Maßen wie die Referenz und hohem PSNR. Bei 12 MP reichen allen 256 MB.
- Abgeschnittene HEIC- und AVIF-Dateien enden bei allen mit einem Fehler, ohne Panik.
- `zune-heic` liefert nativ (macOS, mit VideoToolbox) und in Wasm unterschiedliche Pixel. Alle anderen sind nativ und in Wasm bitgleich.
- `oxideav-avif` lehnt einzelne AV1-Bilder über 35.651.584 Pixel ab, die Grenze der AV1-Level. Größere AVIFs müssen in Kacheln kodiert sein.
- `oxideav-avif` kodiert auch. Ein AVIF mit 1024 px dauert in Wasm 1,1 s, eines mit 256 px 0,2 s. Das ist für spätere AVIF-Thumbnails von Bedeutung.
- Ausgeschlossen sind `heic` und `rav1d-safe` wegen AGPL-3.0, und `av1oxide`, weil es keinen Container-Parser hat und rund 950 `unsafe`-Stellen enthält.

## Varianten

### Wo wird umgewandelt?

| Variante                                                       | Vorteile                                                                                                                                                                                           | Nachteile                                                                                                                                                                 |
| :------------------------------------------------------------- | :------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | :------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| **A. Eigenes Plugin `image-convert` schreibt ein Arbeitsbild** | <ul><li>Dekodieren und Drehen an einer Stelle, alle Bild-Plugins sehen dasselbe Bild.</li><li>Decoder laufen in der Sandbox mit eigenen Grenzen.</li><li>Die Bild-Plugins bleiben klein.</li></ul> | <ul><li>Neues Feld im Dokumentmodell und neue WIT-Version.</li><li>Eine zusätzliche Datei je Foto im Cache.</li><li>Das JPEG-Arbeitsbild ist verlustbehaftet.</li></ul>   |
| B. Jedes Bild-Plugin bindet alle Decoder selbst ein            | <ul><li>Kein neues Feld.</li></ul>                                                                                                                                                                 | <ul><li>Sechs Kopien derselben Decoder, je ≈ 2–3 MB mehr Komponente.</li><li>Jedes Foto wird sechsmal dekodiert.</li><li>Drehung kann wieder auseinanderlaufen.</li></ul> |
| C. Der Host wandelt vor der Pipeline um                        | <ul><li>Kein Plugin nötig.</li></ul>                                                                                                                                                               | <ul><li>Junge Decoder liefen ohne Sandbox und ohne Grenzen im Host (Verfassung, _Security_ 3).</li></ul>                                                                  |

### Format des Arbeitsbilds

| Variante                 | Vorteile                                                                                                                                            | Nachteile                                                                                                        |
| :----------------------- | :-------------------------------------------------------------------------------------------------------------------------------------------------- | :--------------------------------------------------------------------------------------------------------------- |
| **A. JPEG, Qualität 95** | <ul><li>Alle Bild-Plugins lesen JPEG schon.</li><li>`privacy-redact` braucht keinen weiteren Decoder.</li><li>Klein (≈ 1–3 MB bei 12 MP).</li></ul> | <ul><li>Leichter Verlust; für Erkennung und Schwärzung ohne Bedeutung.</li></ul>                                 |
| B. PNG                   | <ul><li>Verlustfrei.</li></ul>                                                                                                                      | <ul><li>Etwa 10- bis 20-mal größer und langsamer zu schreiben.</li><li>`privacy-redact` kann kein PNG.</li></ul> |

## Entscheidung

1. **Arbeitsbild (Variante A).** Das neue Plugin `image-convert` läuft direkt nach `metadata`. Es setzt `doc.working_image` mit Pfad, Quellformat, Breite und Höhe des Arbeitsbilds und einem Merker, ob es umgewandelt hat.
2. **Regel für Bild-Plugins.** Jedes Plugin, das Pixel verarbeitet, liest das Arbeitsbild: `working_image.path`, falls gesetzt, sonst `file.path`. Das sind `thumbnail`, der dHash von `duplicate`, `ai-tagging`, `ocr-text`, `face-detection` und `privacy-redact`. Das SDK bietet dafür eine gemeinsame Funktion an. Rahmen und Koordinaten (Gesichter, Text) beziehen sich auf dieses Bild. Dateibezogene Werte bleiben beim Original: `metadata` liest EXIF und `duplicate` den SHA-256 der Datei.
3. **Inhalt des Arbeitsbilds.**
   - Es ist ein Baseline-JPEG in 8 Bit RGB, Qualität 95 (Setting).
   - Die Drehung ist bereits angewandt. Bei JPEG, PNG, TIFF und WebP folgt sie dem EXIF-Tag Orientation, bei HEIC und AVIF den Container-Transformationen (`irot`, `imir`, `clap`). EXIF-Orientation in HEIF-Dateien wird dabei nicht zusätzlich angewandt.
   - Transparenz wird auf Weiß gelegt.
   - Ein ICC-Profil der Quelle wird übernommen. Andere Metadaten werden nicht übernommen, also kein EXIF, kein GPS und kein XMP.
4. **Kein Umweg für fertige JPEGs.** Ein vollständiges JPEG ohne Drehung (Orientation fehlt oder ist 1) wird nicht neu kodiert. `working_image.path` ist dann `file.path`, ohne Qualitätsverlust und ohne Kopie.
5. **Decoder.**
   - `image` für JPEG, PNG, TIFF, BMP und WebP;
   - `heic-rs` für HEIC/HEIF;
   - `oxideav-avif` für AVIF.

   Alle sind in reinem Rust und laufen im Gast. `heic-rs` und `oxideav-avif` werden mit `=` auf die geprüfte Version festgelegt. Eine neue Version kommt nur mit erneutem Test gegen die Testbilder.

6. **Ablage.** Das Arbeitsbild liegt unter `cache/working/<sha256>.working.jpg`. Der Ordner kommt aus dem Setting `output_dir`, der Schlüssel ist der SHA-256 der Quellbytes. Existiert die Datei schon, wird sie wiederverwendet. Der Watcher ignoriert Dateien auf `.working.jpg`, wie schon `.redacted.jpg`.
7. **Kein Ersatz (ADR 0002).** Ist die Quelle nicht lesbar, nicht dekodierbar, im Format nicht unterstützt oder größer als `max_pixels`, bleibt `working_image = None`, und das Plugin warnt mit Pfad und Ursache. Die Bild-Plugins lesen dann das Original wie bisher, und was sie nicht dekodieren können, liefern sie nicht.
8. **Grenzen.** `image-convert` hat eigene `resource_limits`. Die Werte legt das PRD nach Messung fest; 48 MP brauchen mehr als 512 MB.
9. **WIT.** Das Paket wird zu `wasmgeohost:media@0.3.0` und erhält den Datensatz `working-image`. Host, SDK und Modelle wandeln verlustfrei um (ADR 0003).

## Konsequenzen

**Positiv**

- HEIC-, AVIF-, WebP-, TIFF- und BMP-Fotos werden auf Gesichter geprüft und geschwärzt. Sie bekommen außerdem OCR, Schlagwörter, Thumbnails und einen dHash.
- Alle Bild-Plugins sehen dasselbe, richtig gedrehte Bild. Gesichtsrahmen, Schwärzung und Thumbnails passen zueinander.
- Jedes Foto wird höchstens einmal mit den neuen Decodern dekodiert. Die Bild-Plugins brauchen nur JPEG.
- Das Arbeitsbild enthält keine Ortsdaten, ein späteres Weitergeben gibt kein GPS preis.
- `oxideav-avif` kann später auch AVIF-Thumbnails kodieren.

**Negativ**

- Die Decoder sind erst Wochen bis Monate alt und haben wenige Nutzer. Fehler in seltenen Varianten sind wahrscheinlich, z. B. 10-Bit-HEIC, Alpha und Kachel-AVIF. Die feste Version, die Sandbox und die Tests mit Testbildern begrenzen das Risiko.
- Die Bedeutung der Rahmen in `faces` und `ocr` ändert sich bei gedrehten JPEGs. Sie bezog sich bisher auf das ungedrehte Bild, jetzt auf das gedrehte. Gespeicherte Ergebnisse älterer Läufe passen nicht mehr und müssen neu verarbeitet werden.
- Im Cache liegt je umgewandeltem Foto ein zusätzliches JPEG (≈ 1–3 MB bei 12 MP).
- Bis 12 MP dauert die Umwandlung in Wasm bis zu etwa 0,8 s, bei 48 MP über 2 s, und sie braucht mehr Speicher.
- Neue WIT-Version: Alle Plugins werden neu gebaut.

**Verfassung**

- Keine Regel ändert sich.
- _Architektur_ 3 bleibt gewahrt: Die Bild-Plugins hängen nicht von `image-convert` ab, sondern nur vom Feld `working_image` im `UnifiedDocument` (_Architektur_ 4).
- _Strikte Regeln_ 1 (Rust) und _Security_ 1–3 bleiben gewahrt: Die Decoder sind in Rust, sie laufen im Gast mit Grant `filesystem` und eigenen Grenzen.
