---
feature: face-detection-yunet
status: accepted
archive-after:
touches:
  - plugins/face-detection/**
  - .cargo/config.toml
  - crates/host/tests/**
  - config.yaml
  - config.mac-studio.yaml
  - NOTICE
  - README.md
  - docs/arc42.md
  - CHANGELOG.md
  - Cargo.lock
---

# Tech Spec — Gesichtserkennung mit YuNet (`face-detection-yunet`)

Setzt das PRD `docs/prd/face-detection-yunet.md` um. Die Verfassung hat Vorrang; diese Tech Spec darf sie präzisieren, nie lockern. Grundlage ist [ADR 0009](../adr/0009-plugin-inference-rten.md).

## Architektur

**`plugins/face-detection/Cargo.toml`**

- `rten = { version = "0.28", default-features = false, features = ["onnx_format"] }`
- `rten-tensor = "0.28"`
- `image = { version = "0.25", default-features = false, features = ["jpeg", "png"] }`

**`plugins/face-detection/models/`**

- `face_detection_yunet_2023mar.onnx`: 232.589 Byte, SHA-256 `8f2383e4dd3cfbb4553ea8718107fc0423210dc964f9f4280604804ed2552fa4`.
- `LICENSE`: MIT, Copyright (c) 2020 Shiqi Yu.
- `SOURCE.md`: Quelle (`https://github.com/opencv/opencv_zoo/tree/main/models/face_detection_yunet`), Datei, Größe, SHA-256, Abrufdatum.

**`plugins/face-detection/src/yunet.rs`** (neu)

- `pub static MODEL: &[u8] = include_bytes!("../models/face_detection_yunet_2023mar.onnx");`
- `pub const MODEL_NAME: &str = "yunet-2023mar";`
- Konstanten: `INPUT_SIZE = 640`, `SCORE_THRESHOLD = 0.6`, `NMS_IOU = 0.3`, `STRIDES = [8, 16, 32]`.
- `YuNetDetector::new() -> Result<Self, String>` lädt das Modell mit `rten::Model::load_static_slice(MODEL)` (≈ 2,5 ms in Wasm) und prüft, dass die zwölf Ausgaben `cls_*`, `obj_*`, `bbox_*`, `kps_*` vorhanden sind.
- `impl FaceDetector for YuNetDetector`, `detect(image)`:
  1. **Format:** `image::guess_format`. Nur `Jpeg` und `Png` sind erlaubt, sonst `Err("unsupported image format <fmt>")`. Dekodieren mit `load_from_memory_with_format` und `to_rgb8()`; die EXIF-Drehung wird nicht angewendet.
  2. **Eingabe:** `scale = 640 / max(w, h)`. Das Bild wird mit `FilterType::Triangle` auf `round(w·scale) × round(h·scale)` skaliert und oben links in einen Tensor `[1, 3, 640, 640]` gelegt, als BGR-Float 0..255; der Rest ist 0.
  3. **Lauf:** `model.run` mit allen zwölf Ausgaben.
  4. **Auswertung** je Stride `s` mit `cols = 640 / s`, für jede Zelle `i`:
     - `row = i / cols`, `col = i % cols`;
     - `score = sqrt(clamp(cls) · clamp(obj))`;
     - Mittelpunkt `cx = (col + b0)·s`, `cy = (row + b1)·s`; Größe `w = exp(b2)·s`, `h = exp(b3)·s`;
     - Gesichtspunkte `k = ((col + kx)·s, (row + ky)·s)`.

     Treffer mit `score < 0,6` werden verworfen.

  5. **Non-Maximum-Suppression:** nach `score` absteigend; ein Treffer mit IoU ≥ 0,3 zu einem schon behaltenen Treffer entfällt. Bei gleichem `score` entscheidet der kleinere Index.
  6. **Umrechnen:** Rahmen und Gesichtspunkte werden durch `scale` geteilt und auf Originalbreite bzw. -höhe normiert (0..1, auf 0..1 geklemmt). YuNet liefert die Punkte in der Reihenfolge Augen (2), Nasenspitze, Mundwinkel (2). `FaceLandmarks` bekommt sie in Bildkoordinaten:
     - `left_eye`/`right_eye`: das Auge mit dem kleineren bzw. größeren `x`;
     - `nose`: die Nasenspitze;
     - `mouth_left`/`mouth_right`: der Mundwinkel mit dem kleineren bzw. größeren `x`.
  7. **Sortieren und IDs:** nach `(bbox.y, bbox.x)` sortieren und `face-1` … `face-n` vergeben; `embedding` und `person_id` bleiben `None`.

- Kein `unsafe`, keine Panik: Fehler aus `image` und `rten` werden zu `Err(String)`.

**`plugins/face-detection/src/lib.rs`**

- `FaceDetectionPlugin::new()` versucht `YuNetDetector::new()`.
  - Gelingt es, ist das der Detector.
  - Sonst merkt sich das Plugin den Fehler und warnt in `process` mit `face-detection: built-in detector unavailable: <Fehler>`; `doc.faces` bleibt `None`.
- `with_detector` bleibt unverändert (Tests).
- `FaceDetectionSettings`: Der Grund bei `tier`/`device` (und neu `model`) lautet „only the built-in YuNet detector is available, see ADR 0009“.
- Die bestehende Logik bleibt: lesen per `fs-read`, `min_confidence`, `detection_model`, Warnungen.

**`.cargo/config.toml`** (neu)

```toml
[target.wasm32-wasip2]
rustflags = ["-C", "target-feature=+simd128"]
```

Das gilt für `cargo build --target wasm32-wasip2`, `make components`, `make wasm-cli` und den Komponenten-Build der Host-Tests.

**Konfiguration**

- `config.yaml` und `config.mac-studio.yaml`: `face-detection` steht auf `enabled: true`, der Kommentar „experimental“ entfällt.
- `AppConfig::default()`: ebenfalls `enabled: true`.
- `config.docker.yaml` bleibt unverändert (PRD, Nicht-Ziele der vorigen Features; Docker wird nicht gepflegt).

## Schnittstellen

- WIT und Dokumentmodell: unverändert. `FacesInfo { faces, detection_model: "yunet-2023mar" }`; `DetectedFace` mit `id`, normiertem `bbox`, `confidence` und `landmarks`.
- Settings: `min_confidence` wie bisher. `model`, `tier` und `device` werden als nicht unterstützt gemeldet.

## Datenmodell

Keine Änderung. Gespeicherte Gesichter (`faces`-Tabelle, Schlüssel `<photo_id>/<face.id>`, Feature `faces-unique-storage`) entstehen jetzt aus echten Erkennungen.

## Umsetzung der Akzeptanzkriterien

Tests in `crates/host/tests/face_yunet_test.rs`, außer AC-9 (siehe unten).

| AC    | Umsetzung                          | Test                                                                                                                |
| ----- | ---------------------------------- | ------------------------------------------------------------------------------------------------------------------- |
| AC-1  | `YuNetDetector`                    | `test_ac1_faces_in_public_domain_photos` (Plugin nativ) und `test_ac1_component_matches` (Komponente über den Host) |
| AC-2  | `Some(vec![])`                     | `test_ac2_no_face`                                                                                                  |
| AC-3  | Formatprüfung, Fehler → `None`     | `test_ac3_unreadable_and_unsupported`                                                                               |
| AC-4  | Sortierung, IDs, Determinismus     | `test_ac4_deterministic_ids_and_min_confidence`                                                                     |
| AC-5  | `privacy-redact` mit echten Rahmen | `test_ac5_privacy_redact_redacts_detected_faces`                                                                    |
| AC-6  | `include_bytes!`, `SOURCE.md`      | `test_ac6_embedded_model_and_attribution`                                                                           |
| AC-7  | Standardgrenzen                    | `test_ac7_large_image_within_default_limits` (4000 × 3000 aus dem Apollo-Bild hochskaliert)                         |
| AC-8  | Konfigurationen                    | `test_ac8_enabled_in_shipped_configs` (prüft auch, dass `process` mit `config.yaml` nicht mehr warnt)               |
| AC-9  | `.cargo/config.toml`               | `test_ac9_simd_for_components` (Dateiinhalt plus `wasm-tools`-freie Prüfung über das Laden aller Komponenten)       |
| AC-10 | `SOURCES.md`                       | `test_ac10_fixture_sources`                                                                                         |

**Testbilder** in `crates/host/tests/fixtures/faces/`:

- `apollo11.jpg`: 1600 × 1257, 538.132 Byte, SHA-256 `fb6528862bd411a5057d446a7530b00ff15b3c84896903c5c40a5ff0e2ba03c7`.
- `expedition64.jpg`: 1600 × 1280, 708.818 Byte, SHA-256 `a07491a39e07cc4ee671eb8d1704beabbe7113a10a8e34b2c73841c7f210a5f3`.
- `SOURCES.md` mit beiden Quellen.

**Anpassungen bestehender Tests** (die Kennungen bleiben):

- `no_synthetic_test.rs` `test_ac3_no_engine_no_result` (`no-synthetic-output` AC-3): Die Erwartung „no engine → `None`“ gilt nur noch für `ocr-text` und `ai-tagging`. Für `face-detection` prüft der Test jetzt, dass ein Bild ohne Gesicht `Some(vec![])` ergibt.
- `no_synthetic_test.rs` `test_ac6_shipped_configs_disable_experimental_plugins`: `face-detection` muss in `config.yaml`, `config.mac-studio.yaml` und `AppConfig::default()` an sein; `config.docker.yaml` wird nicht mehr geprüft.
- Der Warnungstext zu `tier`/`device` ändert sich: „only the built-in YuNet detector is available, see ADR 0009“.
- `face_test.rs`: Die Tests mit Test-Detector bleiben. Ein Test, der `new()` ohne Detector voraussetzt, wird angepasst.

## Sicherheit

- _Strikte Regeln_ 1: `rten`, `rten-tensor` und `image` sind Rust; kein `unsafe` im Plugin. `Model::load_static_slice` ist eine sichere API, das unsichere `load_mmap` wird nicht genutzt.
- _Strikte Regeln_ 2: Gesichter stammen nur aus dem Modell. Scheitert etwas, ist das Ergebnis `None` mit Warnung.
- _Security_ 1–3: kein neuer Grant (nur `filesystem` wie bisher); Speicher und Zeit sind durch `resource_limits` begrenzt (AC-7).
- Lizenz: Modell (MIT) und Testbilder (Public Domain) sind mit Quelle abgelegt; `NOTICE` nennt YuNet.

## Messwerte

Gemessen am 2026-11-27 auf macOS (Apple Silicon), Release-Build, Wasm-SIMD.

| Messung | Wert |
| :-- | --: |
| Komponente `flagme_plugin_face_detection.wasm` | ≈ 0,34 MB → 5.073.534 Byte |
| `face-detection` über den Host, 1600 × 1257 (`apollo11.jpg`), inkl. Instanziierung, Modell laden, Dekodieren | 136–138 ms |
| dasselbe, 4000 × 3000 (Test AC-7) | 255 ms (Debug-Host: 685 ms) |
| höchster Speicher des ganzen Host-Prozesses, `expedition64.jpg` | 84 MB |

Ergänzung zur Umsetzung: Der JPEG-Dekoder von `image` füllt fehlende Bilddaten eines abgeschnittenen JPEG ohne Fehler grau auf. `YuNetDetector` lehnt deshalb ein JPEG ab, wenn nach dem ersten SOS-Marker (`FF DA`) kein EOI-Marker (`FF D9`) folgt. Geprüft an allen 25 JPEGs in `Examples/` und den Testbildern: keines wird fälschlich abgelehnt.

## Offene Punkte

- Keine.
