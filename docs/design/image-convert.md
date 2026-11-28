---
feature: image-convert
status: accepted
archive-after:
touches:
  - plugins/image-convert/**
  - plugins/face-detection/**
  - plugins/privacy-redact/**
  - plugins/ocr-text/**
  - plugins/ai-tagging/**
  - plugins/thumbnail/**
  - plugins/duplicate/**
  - crates/models/**
  - crates/sdk/**
  - crates/host/src/**
  - crates/host/tests/**
  - wit/**
  - Cargo.toml
  - Cargo.lock
  - Makefile
  - config*.yaml
  - NOTICE
  - README.md
  - CHANGELOG.md
  - docs/arc42.md
  - docs/pipeline_host_supervisor.md
  - docs/plugin-development-guide.md
---

# Tech Spec — Arbeitsbild für alle Eingabeformate (`image-convert`)

Setzt das PRD `docs/prd/image-convert.md` um. Die Verfassung hat Vorrang; diese Tech Spec darf sie präzisieren, nie lockern. Grundlage ist [ADR 0011](../adr/0011-working-image-pure-rust-decoders.md).

## Architektur

```
metadata ─▶ image-convert ─▶ reversegeo … thumbnail ─▶ ai-tagging ─▶ duplicate … ocr-text ─▶ face-detection ─▶ privacy-redact
   │              │                          │              │             │           │              │                │
 file.path    file.path → working_image   pixel_source  pixel_source  file.path (SHA-256)          pixel_source      pixel_source
 (EXIF)       (fs-read, fs-write)                                     + pixel_source (dHash)
```

**`plugins/image-convert`** (neu, Aufbau wie die anderen Plugins: `lib.rs`, `main.rs`, Feature `component`, `crate-type = ["cdylib", "rlib"]`)

- Abhängigkeiten:
  - `flagme-sdk`;
  - `image = { version = "0.25", default-features = false, features = ["jpeg", "png", "tiff", "bmp", "webp"] }`;
  - `heic-rs = { version = "=0.1.5", default-features = false, features = ["std"] }`, also ohne `parallel` und ohne `rayon`;
  - `oxideav-avif = { version = "=0.0.18", default-features = false, features = ["registry"] }`. `registry` zieht den AV1-Decoder `oxideav-av1` nach; es wird nur `decode` genutzt;
  - `sha2 = "0.10"`.
- `src/detect.rs`: `SourceFormat::sniff(bytes) -> Option<SourceFormat>` anhand der Signatur, nicht der Endung:

  | Format | Signatur                                                                                                              |
  | :----- | :-------------------------------------------------------------------------------------------------------------------- |
  | `Jpeg` | `FF D8 FF`                                                                                                            |
  | `Png`  | `89 50 4E 47 0D 0A 1A 0A`                                                                                             |
  | `Tiff` | `49 49 2A 00` oder `4D 4D 00 2A`                                                                                      |
  | `Bmp`  | `42 4D`                                                                                                               |
  | `Webp` | `RIFF` … `WEBP` an Byte 8                                                                                             |
  | `Heic` | `ftyp` an Byte 4, Major- oder kompatible Marke aus `heic`, `heix`, `heim`, `heis`, `hevc`, `hevx`, `mif1` ohne `avif` |
  | `Avif` | `ftyp` an Byte 4, Marke `avif` oder `avis`                                                                            |

  `as_str()` liefert `jpeg`, `png`, `tiff`, `bmp`, `webp`, `heic`, `avif`. Alles andere, z. B. GIF, ergibt `None`.

- `src/convert.rs`:
  - `pub fn probe(bytes, fmt) -> Result<(u32, u32), String>` liefert die Maße nach der Drehung, ohne zu dekodieren:
    - `image`: `ImageReader::into_dimensions`, bei Orientation 5–8 getauscht;
    - HEIC: `heic_rs::probe`, das `width`/`height` schon nach `irot` meldet;
    - AVIF: `oxideav_avif::info`, bei `irot` 90°/270° getauscht.
  - `pub fn decode_oriented(bytes, fmt) -> Result<Decoded, String>` mit `Decoded { rgb: RgbImage, icc: Option<Vec<u8>> }`:
    - **`image`:** `ImageReader::with_format(..).into_decoder()`, dann `orientation()` und `icc_profile()` lesen, `DynamicImage::from_decoder`, `apply_orientation`;
    - **HEIC:** `heic_rs::decode(bytes, &DecodeOptions { layout: Rgba8, apply_transforms: true, threads: Some(1), ..Default })`. Das ICC-Profil kommt aus `heic_rs::context` (`props(primary_item)` → `icc`). EXIF-Orientation wird nicht angewandt (ADR 0011, Punkt 3);
    - **AVIF:** `oxideav_avif::decode` → `to_rgba8()`, ICC aus `metadata.icc`. Die Container-Transformationen wendet `oxideav-avif` selbst an (ADR 0011, Prototyp);
    - **Alpha:** RGBA wird auf Weiß gelegt, `c' = (c·a + 255·(255−a)) / 255` je Kanal, gerundet;
    - **Tiefe:** 16-Bit-Quellen (PNG, TIFF, 10-Bit-AVIF/HEIC) werden über `to_rgb8` bzw. `Rgba8` auf 8 Bit gebracht.
  - `pub fn encode_jpeg(&Decoded, quality) -> Result<Vec<u8>, String>`: `JpegEncoder::new_with_quality`, `set_icc_profile` bei vorhandenem Profil, `encode_image`. Es schreibt keine APP1-Daten (EXIF/XMP).
  - `pub fn jpeg_needs_conversion(bytes) -> bool`. Es ergibt `true`, wenn:
    - der EOI-Marker fehlt (Prüfung wie `jpeg_is_complete` in `face-detection`, kopiert nach _Architektur/Layering_ 3);
    - oder die EXIF-Orientation ≠ 1 ist (`image`-Decoder `orientation()`).
- `src/lib.rs`:
  - `ImageConvertSettings { output_dir: "cache/working", quality: 95, max_pixels: 50_000_000 }`;
  - `quality` mit `r.u32(.., 1..=100, ..)`, `max_pixels` mit `r.u32(.., 1_000_000..=100_000_000, ..)`, `output_dir` mit `r.path`.
  - `process(doc)`:
    1. `bytes = fs_read(file.path)`. Bei einem Fehler: Warnung `image-convert: cannot read '<path>': <e>; no working image` und `Ok(doc)`.
    2. `fmt = sniff(bytes)`. Bei `None`: Warnung `… unsupported format …`.
    3. `fmt == Jpeg && !jpeg_needs_conversion`: `working_image = { path: file.path, source_format: "jpeg", width, height (probe), converted: false }`.
    4. `(w, h) = probe`. Bei `w·h > max_pixels`: Warnung `… <w>×<h> exceeds max_pixels <n> …`.
    5. `key = hex(sha256(bytes))`, `out = <output_dir>/<key>.working.jpg`.
    6. Ist `fs_read(out)` erfolgreich und ein JPEG mit den Maßen `(w, h)`, wird die Datei wiederverwendet (AC-7).
    7. Sonst `decode_oriented` → `encode_jpeg` → `fs_write(out)`.
    8. `working_image = { path: out, source_format: fmt, width: w, height: h, converted: true }`.

    Jeder Fehler in 4–7 führt zu einer Warnung mit Pfad und Ursache, `working_image` bleibt `None` und es wird nichts geschrieben. Panics der Decoder fängt die Sandbox ab: Die Komponente endet mit einem Trap, der Host warnt, und die Pipeline läuft mit dem unveränderten Dokument weiter (ADR 0003).

- `main.rs`: CLI wie bei den anderen Plugins (`flagme-plugin-image-convert`, JSON über stdin/stdout).

**SDK** (`crates/sdk/src/lib.rs`)

- `pub fn pixel_source(doc: &UnifiedDocument) -> &str` liefert `doc.working_image.as_ref().map_or(&doc.file.path, |w| &w.path)`.
- Die Umwandlung `document_to_wit` / `document_from_wit` bekommt das Feld `working_image`.

**Bild-Plugins** (nur der gelesene Pfad ändert sich)

| Plugin                                     | Stelle                                        | neu                                                                                                                    |
| :----------------------------------------- | :-------------------------------------------- | :--------------------------------------------------------------------------------------------------------------------- |
| `thumbnail`                                | `process`, `let path = doc.file.path.clone()` | `pixel_source(&doc)`; `calculate_cache_key` nutzt weiter `file.path`                                                   |
| `duplicate`                                | dHash                                         | Bytes von `pixel_source(&doc)`; der SHA-256 bleibt über `file.path`. Sind beide Pfade gleich, wird nur einmal gelesen. |
| `ai-tagging`, `ocr-text`, `face-detection` | `fs_read(&doc.file.path)`                     | `fs_read(pixel_source(&doc))`; Warnungen nennen den gelesenen Pfad                                                     |
| `privacy-redact`                           | `write_redacted_image(source_path = …)`       | `pixel_source(&doc)`; `redact_jpeg` bleibt bei JPEG                                                                    |

**Host**

- `component.rs`: Umwandlung Host ↔ WIT für `working_image` in beiden Richtungen.
- `config.rs` (`AppConfig::default()`): Plugin `image-convert` mit `enabled: true`, `capabilities: ["filesystem"]`, `resource_limits { max_memory_bytes: 1073741824, timeout_ms: 30000 }`; `pipeline` mit `image-convert` an zweiter Stelle.
- `watcher.rs`:
  - `is_supported_media_file` nimmt zusätzlich `bmp` und `heif` an und übergeht Dateinamen auf `.working.jpg`;
  - `detect_mime_type` gibt `bmp` als `image/bmp` und `heif` als `image/heif` aus.
- Die Speicherung (`storage.rs`) bleibt unverändert (PRD, Nicht-Ziele).

**Build und Konfiguration**

- `Cargo.toml` (Workspace): Mitglied `plugins/image-convert`. Für `heic-rs`, `oxideav-av1` und `oxideav-avif` gilt `[profile.dev.package.<name>] opt-level = 3`, damit Tests im Debug-Build nicht minutenlang dekodieren, wie bei `rten`.
- `Makefile`: `image-convert` in `PLUGINS`, direkt nach `metadata`.
- `config.yaml` und `config_mac-studio_32GB.yaml`: `image-convert` in `pipeline` nach `metadata`, im Abschnitt `plugins` mit Kommentar `# working image for all formats (ADR 0011)`, `enabled: true`, `capabilities: [filesystem]`, `resource_limits` wie oben.

## Schnittstellen

- **WIT** (`wit/media-plugin.wit`): `package wasmgeohost:media@0.3.0`, dazu

  ```wit
  /// Image the pixel plugins read (ADR 0011): oriented baseline JPEG, or the original JPEG.
  record working-image {
      path: string,
      /// jpeg, png, tiff, bmp, webp, heic or avif
      source-format: string,
      width: u32,
      height: u32,
      /// false: `path` is the original file (complete JPEG without rotation)
      converted: bool,
  }
  ```

  Im `record document` steht das Feld `working-image: option<working-image>` nach `file`. Alle Komponenten werden neu gebaut, ältere Komponenten mit `@0.2.0` lädt der Host nicht mehr.

- **SDK:** `flagme_sdk::pixel_source`.
- **Settings `image-convert`:** `output_dir` (Pfad), `quality` (1–100), `max_pixels` (1.000.000–100.000.000).
- **CLI:** keine Änderung. `flagme-host process|stream` gibt `working_image` im JSON aus.

## Datenmodell

`crates/models/src/lib.rs`:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkingImage {
    pub path: String,
    pub source_format: String,
    pub width: u32,
    pub height: u32,
    pub converted: bool,
}
```

In `UnifiedDocument` kommt nach `file` das Feld `#[serde(default, skip_serializing_if = "Option::is_none")] pub working_image: Option<WorkingImage>`. `UnifiedDocument::new` setzt es auf `None`. Ältere JSON-Dokumente bleiben lesbar. Es gibt keine Datenbankmigration.

## Umsetzung der Akzeptanzkriterien

Plugin-Tests (nativ) stehen in `plugins/image-convert/tests/convert_test.rs`, Host-Tests in `crates/host/tests/image_convert_test.rs`. Gemeinsame Hilfen kommen nach `common`: `formats_fixture(name)` und `psnr(a, b)`.

| AC    | Umsetzung                                               | Test                                                                                                                                                                                                  |
| :---- | :------------------------------------------------------ | :---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| AC-1  | `sniff`, `decode_oriented`, `encode_jpeg`               | `test_ac1_all_formats_to_jpeg`                                                                                                                                                                        |
| AC-2  | `jpeg_needs_conversion`                                 | `test_ac2_plain_jpeg_passthrough`                                                                                                                                                                     |
| AC-3  | `apply_orientation`; `irot` in `heic-rs`/`oxideav-avif` | `test_ac3_rotation_exif_and_irot`                                                                                                                                                                     |
| AC-4  | Alpha auf Weiß                                          | `test_ac4_alpha_on_white`                                                                                                                                                                             |
| AC-5  | `encode_jpeg` ohne APP1, mit ICC                        | `test_ac5_no_metadata_but_icc` (Marker im JPEG prüfen; ICC-Quelle: PNG mit `iCCP` und HEIC mit `colr prof`)                                                                                           |
| AC-6  | Fehlerpfade in `process`                                | `test_ac6_failures_leave_none` (unlesbar, `apollo11-trunc.heic`, `apollo11-trunc.avif`, GIF, `max_pixels` 1.000.000 gegen 800 × 629 × 2)                                                              |
| AC-7  | Wiederverwendung, Dateiname                             | `test_ac7_cache_reuse` (Änderungszeit und Inhalt gleich, kein zweites `fs_write`)                                                                                                                     |
| AC-8  | Host-Pipeline                                           | `test_ac8_faces_redacted_for_heic_avif_webp` (Komponenten)                                                                                                                                            |
| AC-9  | `pixel_source` in sechs Plugins                         | `test_ac9_pixel_plugins_read_working_image`: Dokument mit `working_image` auf ein anderes Bild. Geprüft wird, dass die Ergebnisse diesem Bild folgen, und mit `LogCaps`, welche Pfade gelesen wurden. |
| AC-10 | Gedrehtes JPEG                                          | `test_ac10_rotated_jpeg_faces_and_redaction` (Rahmen innerhalb von 629 × 800; der geschwärzte Bereich weicht an den Rahmen stark vom Arbeitsbild ab, außerhalb nicht)                                 |
| AC-11 | Konfiguration, Grenzen                                  | `test_ac11_enabled_first_after_metadata`, `test_ac11_48mp_heic_within_limits` (Testbild `gradient-48mp.heic` über den Host mit den Grenzen der Konfiguration)                                         |
| AC-12 | WIT-Umwandlung                                          | `test_ac12_working_image_roundtrip` (Probe-Komponente und JSON)                                                                                                                                       |
| AC-13 | Watcher                                                 | `test_ac13_bmp_and_working_jpg`                                                                                                                                                                       |
| AC-14 | Settings                                                | `test_ac14_invalid_settings`                                                                                                                                                                          |
| AC-15 | Dokumentation                                           | `test_ac15_sources_and_notice` (SHA-256 aller Dateien in `formats/` gegen `SOURCES.md`; `NOTICE` und README nennen beide Crates)                                                                      |

**Testbilder** (`crates/host/tests/fixtures/formats/`, Quelle `faces/apollo11.jpg`, ImageMagick 7 mit libheif 1.23.7; Befehle in `SOURCES.md`):

| Datei                            | Erzeugung                                                                                             |
| :------------------------------- | :---------------------------------------------------------------------------------------------------- |
| `apollo11.png`                   | `magick apollo11.jpg -resize 800x apollo11.png` (Referenz, 800 × 629)                                 |
| `apollo11.tiff`, `.bmp`, `.webp` | aus `apollo11.png`; BMP als `BMP3:`                                                                   |
| `apollo11.heic`, `apollo11.avif` | aus `apollo11.png`, `-quality 80`                                                                     |
| `apollo11-10bit.avif`            | `-depth 10 -quality 80`                                                                               |
| `apollo11-alpha.png`             | `apollo11.png` mit transparentem linkem Viertel (`-alpha set -region 200x629+0+0 -alpha transparent`) |
| `apollo11-icc.png`               | `apollo11.png` mit sRGB-Profil (`-profile sRGB.icc`, Profil aus ImageMagick)                          |
| `apollo11-rot90.jpg`             | `apollo11.png` mit `-orient RightTop`, Pixel ungedreht                                                |
| `apollo11-rot90.heic`, `.avif`   | wie oben; libheif schreibt `irot` 270°                                                                |
| `apollo11-trunc.heic`, `.avif`   | die ersten 60 % der Bytes                                                                             |
| `apollo11.gif`                   | nicht unterstütztes Format                                                                            |
| `gradient-48mp.heic`             | `magick -size 8064x6048 gradient:navy-orange -quality 50` (≈ 13 KB, für AC-11)                        |

Zusammen sollen es ≤ 3 MB sein.

**Anpassungen bestehender Tests** (Kennungen bleiben):

- `component_runtime_test.rs`, `ac_test.rs`, `examples_test.rs`: `plugin_names()` und die Pipeline-Reihenfolge enthalten `image-convert`. Die Beispielausgabe enthält `working_image` mit `converted: false` für `Examples/sample.jpg`.
- `no_synthetic_test.rs`: `Examples/sample.heic` ist kein gültiges HEIF und ergibt weiter keine Ausgabe. Die erwarteten Warnungen kommen jetzt auch von `image-convert`.
- `settings_test.rs`: Standardwerte von `image-convert`.
- `face_yunet_test.rs`, `ocr_paddle_test.rs`, `clip_test.rs`: Die Tests ohne `working_image` bleiben gültig (AC-9, Rückfall auf `file.path`).

## Sicherheit

- **_Security_ 1–3:** `image-convert` bekommt nur den Grant `filesystem`, wie die anderen Bild-Plugins. Die jungen Decoder laufen ausschließlich im Gast und haben eigene Grenzen von 1 GB und 30 s. Eine präparierte Datei kann höchstens die Komponente zum Absturz bringen, nicht den Host.
- **`max_pixels`:** Die Prüfung vor dem Dekodieren verhindert Dekompressionsbomben, also große Maße in kleinen Dateien. Darunter fängt die Speichergrenze der Sandbox den Rest ab.
- **_Strikte Regeln_ 1:** Alles ist Rust. `heic-rs` hat `#![forbid(unsafe_code)]`, `oxideav-avif` enthält kein `unsafe`. `cargo tree -p flagme-plugin-image-convert` darf kein `-sys`-Crate und keinen C-Build (`cc`) enthalten. Das prüft `test_ac15_sources_and_notice`.
- **_Strikte Regeln_ 2:** Kein Ersatzbild. Fehlt ein Arbeitsbild, lesen die Bild-Plugins das Original wie bisher.
- **Datenschutz:** Das Arbeitsbild enthält kein EXIF und kein GPS (AC-5). Es liegt im Cache wie die Thumbnails und das geschwärzte Bild. Es ist **nicht** geschwärzt und so schutzbedürftig wie das Original.
- **Pfade:** Der Dateiname ist der hexadezimale SHA-256, aus Eingabedaten entsteht kein Pfadanteil. `output_dir` kommt aus der Konfiguration, `fs-write` lehnt `..` ab.

## Ergänzungen zur Umsetzung

- **Tests:** Alle Tests stehen in `crates/host/tests/image_convert_test.rs`, auch die nativen Plugin-Tests, nicht in `plugins/image-convert/tests/`. Die Testbilder liegen unter `crates/host/tests`, und der Host bindet die Plugins dort schon als Dev-Abhängigkeit ein. `common::LogCaps` zeichnet dafür die Pfade von `fs_read` und `fs_write` auf (AC-7, AC-9).
- **Abgeschnittene JPEGs werden abgelehnt, nicht umgewandelt.** Der JPEG-Decoder füllt fehlende Scan-Daten mit Grau, statt einen Fehler zu melden, also wäre ein umgewandeltes Arbeitsbild ein erfundenes Bild. AC-6 prüft das zusätzlich. `jpeg_needs_conversion` ist deshalb entfallen: Ein vollständiges JPEG mit Orientation 1 oder ohne Orientation wird durchgereicht, mit anderer Orientation umgewandelt.
- **Pixelgrenze ohne Drehung:** `probe` heißt `pixel_count` und liefert nur die Pixelzahl aus dem Dateikopf. Für `max_pixels` genügt sie, weil eine Drehung sie nicht ändert. Breite und Höhe in `working_image` kommen aus dem dekodierten Bild bzw. dem Kopf des vorhandenen Arbeitsbilds.
- **AVIF-Farbmatrix:** `try_to_rgba8`. Bei einer Matrix ohne exakten Kern (`Unsupported`) wird auf `to_rgba8` zurückgegriffen, den dokumentierten BT.601-Ersatz der Crate.
- **AC-14 (PRD angepasst):** Zahlen außerhalb des Bereichs werden wie in allen Plugins mit Warnung auf die Bereichsgrenze gesetzt (`SettingsReader::u32`), nicht durch den Standard ersetzt. Nur nicht lesbare Werte ergeben den Standard. `quality` hat den Alias `jpeg_quality`.
- **Watcher:** `is_supported_media_file` und `detect_mime_type` sind jetzt `pub`, damit AC-13 sie direkt prüfen kann.
- **Testbilder:**
  - Die längste Seite ist 640 px statt 800 px, sonst wären es über 3 MB; TIFF ist mit Deflate komprimiert.
  - `apollo11-alpha.png` und die ICC-Testbilder haben 160 px.
  - Das ICC-Profil `test-gamma22.icc` ist selbst erzeugt (`make_icc.py`, CC0), weil auf dem System kein frei weiterverteilbares Profil lag.
  - `generate.sh` erzeugt alle Dateien neu. Zusammen sind es 3,1 MB.
- **`config_mac-studio_32GB.yaml`** (vormals `config.mac-studio.yaml`): Dieses Profil verarbeitet Dateien über 100 MP (siehe `thumbnail`). `image-convert` bekommt dort deshalb 4 GB, 120 s und `max_pixels: '100000000'`.
- **Profile (Wunsch des Maintainers vom 2026-11-28):**
  - `config.docker.yaml` heißt jetzt `config_2cores_4GB.yaml` und wird wieder gepflegt;
  - `config.mac-studio.yaml` heißt jetzt `config_mac-studio_32GB.yaml`;
  - neu sind `config_cpu_8GB.yaml` und `config_mac-studio_64GB.yaml` (Mac Studio M5 Max).

  Alle Profile schalten jedes Plugin ein und nennen die empfohlene Zahl der Watcher-Worker. `common::PROFILES` listet sie für die Tests; `plugin_limits_test` prüft je Profil die eigenen Grenzen. `make dist` kopiert alle fünf. `Deployments/` (Docker) wird nicht gepflegt und nennt noch `config.docker.yaml`.

## Messwerte

Gemessen am 2026-11-28 auf macOS (Apple Silicon), Release-Build der Komponenten, Wasm-SIMD.

| Messung | Wert |
| :-- | --: |
| `flagme_plugin_image_convert.wasm` | 3.752.615 Byte |
| `flagme-host stream` mit `config.yaml`, ganze Pipeline, `apollo11.heic` (640 × 503) | 2,37 s, 485 MB RSS des Host-Prozesses |
| `flagme-host stream`, HEIC eines Galaxy S23 Ultra (1868 × 4000, 4 × 8 Kacheln, `irot` 270°; Foto des Maintainers, nur lokal) | 3,89 s, 485 MB RSS; 2 Gesichter erkannt und geschwärzt |
| Umwandlung je Testbild nativ im Debug-Test (Decoder mit `opt-level = 3`), AC-1 | 95–284 ms (PNG 95, TIFF 125, BMP 219, WebP 284, HEIC 152, AVIF 149, 10-Bit-AVIF 238; 640 × 503) |
| 48-MP-HEIC über den Host (AC-11, Grenzen 1 GB, 30 s) | 1,57 s (8064 × 6048, Release-Komponente) |
| ganze Pipeline mit `apollo11.heic` | 3 Gesichter, 3 Schwärzungen, Thumbnails 128/512/640 px, dHash, 3 Schlagwörter, 2 OCR-Zeilen |

## Offene Punkte

- Keine. Das 48-MP-Testbild ist ein Farbverlauf als HEIC (8064 × 6048, 12.979 Byte); `heic-rs` dekodiert es im Prototyp in 0,45 s.
