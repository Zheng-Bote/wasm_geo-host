---
feature: ocr-text-paddle
status: accepted
archive-after:
touches:
  - plugins/ocr-text/**
  - crates/host/tests/**
  - crates/host/src/config.rs
  - config.yaml
  - config.mac-studio.yaml
  - NOTICE
  - README.md
  - docs/arc42.md
  - docs/pipeline_host_supervisor.md
  - CHANGELOG.md
  - Cargo.lock
---

# Tech Spec — Texterkennung mit PP-OCRv5 (`ocr-text-paddle`)

Setzt das PRD `docs/prd/ocr-text-paddle.md` um. Die Verfassung hat Vorrang; diese Tech Spec darf sie präzisieren, nie lockern. Grundlage ist [ADR 0009](../adr/0009-plugin-inference-rten.md).

## Architektur

**`plugins/ocr-text/Cargo.toml`**

- `rten = { version = "0.28", default-features = false, features = ["onnx_format"] }`
- `rten-tensor = "0.28"`
- `image = { version = "0.25", default-features = false, features = ["jpeg", "png"] }`

**`plugins/ocr-text/models/`**

| Datei                            | Herkunft (Hugging Face)                                         |      Byte | SHA-256                                                            |
| :------------------------------- | :-------------------------------------------------------------- | --------: | :----------------------------------------------------------------- |
| `PP-OCRv5_mobile_det.onnx`       | `PaddlePaddle/PP-OCRv5_mobile_det_onnx`, `inference.onnx`       | 4.826.518 | `a431985659dc921974177a95adcfbb90fd9e51989a5e04d70d0b75f597b6e61d` |
| `PP-OCRv5_mobile_det.yml`        | dito, `inference.yml`                                           |       903 | `98069072e1b6b37d727fd9d9f11725faa46d6ea0de012f2ed26caea011c37699` |
| `latin_PP-OCRv5_mobile_rec.onnx` | `PaddlePaddle/latin_PP-OCRv5_mobile_rec_onnx`, `inference.onnx` | 8.042.023 | `7888113072263cb471b93f66dd5e2ad70548dc526fa1ace760d0d973dd121498` |
| `latin_PP-OCRv5_mobile_rec.yml`  | dito, `inference.yml`                                           |     6.817 | `0bbe984570f597af3638e50bdf2e8276f3ab26a61966096538b3b0d1849f5c84` |

Dazu kommen `LICENSE` (Text der Apache License 2.0; die Modellkarten nennen `license: apache-2.0`, Urheber PaddlePaddle Authors) und `SOURCE.md` (URLs, Abrufdatum, Größen, SHA-256).

**`plugins/ocr-text/src/paddle.rs`** (neu)

- Eingebettet: `DET_MODEL`, `REC_MODEL` (`include_bytes!`) und `REC_CONFIG` (`include_str!` der `.yml`).
- `pub const ENGINE_NAME: &str = "paddleocr-v5-mobile-latin";`
- Konstanten:
  - `DET_LONG_SIDE = 960`, `DET_THRESH = 0.3`, `DET_BOX_THRESH = 0.6`, `DET_UNCLIP = 1.5`, `DET_MIN_SIDE = 3` (px in der Wahrscheinlichkeitskarte);
  - `REC_HEIGHT = 48`, `REC_MIN_WIDTH = 320`;
  - `LINE_MIN_V_OVERLAP = 0.5`, `LINE_MAX_GAP = 0.8` (in Zeilenhöhen).
- **`load_dictionary(REC_CONFIG)`** liest unter `character_dict:` die Einträge `- <zeichen>` ein. Behandelt werden einfache und doppelte Anführungszeichen (`'''` steht für `'`). Ein Verzeichnis mit anderer Länge als 836 Einträgen ist ein Fehler.
- **`PaddleOcrEngine::new() -> Result<Self, String>`** lädt beide Modelle mit `Model::load_static_slice` (≈ 25 ms in Wasm) und das Verzeichnis. Die Zahl der Klassen des Erkennungsmodells muss `836 + 2` sein (Leerzeichen-Index 0, Leerzeichen am Ende).
- **`recognize(image)`**:
  1. **Dekodieren** wie `face-detection`: `guess_format`, nur JPEG und PNG, ein JPEG ohne EOI nach dem SOS gilt als abgeschnitten, `to_rgb8()`, keine EXIF-Drehung. Die Prüffunktion wird kopiert, nicht geteilt, denn Plugins dürfen nicht voneinander abhängen (_Architektur/Layering_ 3).
  2. **Textbereiche finden:**
     - `ratio = 960 / max(w, h)`; Zielgröße je Seite `round(seite · ratio / 32) · 32`, mindestens 32;
     - BGR, `(x/255 − mean)/std` mit mean `0.485, 0.456, 0.406` und std `0.229, 0.224, 0.225`;
     - Ausgabe ist die Karte `[1, 1, H, W]`.
  3. **Bereiche:**
     - Pixel > 0,3, zusammenhängend über 4 Nachbarn (Flutfüllung mit Stapel), ergeben Komponenten;
     - je Komponente das umschließende Rechteck und die mittlere Wahrscheinlichkeit ihrer Pixel;
     - verworfen wird, was eine Wahrscheinlichkeit < 0,6 oder eine kürzere Seite < 3 hat;
     - Erweiterung um `d = Fläche · 1,5 / Umfang` an jeder Seite;
     - Rückrechnung auf das Original, auf das Bild geklemmt.
  4. **Zeilen:**
     - Sortierung nach `y_mitte`;
     - zwei Bereiche gehören zu einer Zeile, wenn sie sich senkrecht zu ≥ 50 % der kleineren Höhe überdecken und der waagrechte Abstand ≤ 0,8 · Zeilenhöhe ist;
     - eine Zeile ist die Vereinigung ihrer Rechtecke;
     - die Zeilen werden nach `y0` gruppiert (gleiche Zeile, wenn die Überdeckung ≥ 50 % ist) und dann nach `x0` sortiert.
  5. **Grenze:** Gibt es mehr als `max_regions` Zeilen, werden die mit der höchsten Bereichswahrscheinlichkeit gelesen. Die Engine meldet das über `RecognizeOutcome { info, truncated_from: Option<usize> }`, und das Plugin warnt.
  6. **Lesen** je Zeile:
     - Ausschnitt auf Höhe 48 skalieren, Breite `ceil(48 · w/h)`;
     - Tensorbreite `max(320, Breite)`, rechts mit 0 aufgefüllt;
     - BGR, `(x/255 − 0,5)/0,5`;
     - Ausgabe `[1, T, 838]`; CTC gierig: Argmax je Schritt, Wiederholungen zusammenfassen, Index 0 überspringen, `i − 1` im Verzeichnis, der letzte Index ist `' '`;
     - Sicherheit = Mittelwert der Argmax-Wahrscheinlichkeiten der ausgegebenen Zeichen;
     - Text und Leerzeichen an den Rändern werden entfernt, leere Zeilen entfallen.
  7. **`OcrInfo`:**
     - `regions` in Leserichtung mit auf 0..1 normiertem `bbox`;
     - `combined_text` = Texte durch `\n` verbunden;
     - `primary_language = None`.
- Kein `unsafe`, keine Panik; jeder Fehler wird zu `Err(String)`.

**`plugins/ocr-text/src/lib.rs`**

- `OcrEngine` bekommt die Methode `recognize_limited(image, max_regions) -> Result<(OcrInfo, Option<usize>), String>`. Die Standard-Implementierung ruft `recognize` auf und liefert `None`, so bleiben Test-Engines unverändert.
- `OcrSettings`:
  - `min_confidence` mit Standard **0,5**;
  - neu `max_regions: u32` (Standard 200, Bereich `1..=1000`);
  - der Grund bei `engine` lautet „only the built-in PaddleOCR engine is available, see ADR 0009“.
- `OcrTextPlugin::new()` nutzt `PaddleOcrEngine`. Scheitert das, warnt das Plugin mit „built-in OCR engine unavailable: …“, und `doc.ocr` bleibt `None`.
- `filter_by_confidence` setzt `combined_text` neu mit `\n` (bisher Leerzeichen) und lässt `primary_language` unverändert.
- Neue Warnung bei Erreichen der Grenze: `ocr-text: '<path>': <n> text lines found, read the <max_regions> most confident (max_regions)`.

**Konfiguration**

- `config.yaml`, `config.mac-studio.yaml`: `ocr-text` mit `enabled: true` und

  ```yaml
  resource_limits:
    timeout_ms: 30000
  ```

  der Kommentar „experimental“ entfällt.

- `AppConfig::default()`: `enabled: true`, `resource_limits` `{timeout_ms: 30000}`.

**Testbilder** in `crates/host/tests/fixtures/ocr/`:

- `german-text.png` (1400 × 420): mit ImageMagick und `-strip` erzeugt, damit keine Zeitstempel in der Datei stehen; vier Zeilen. Der Befehl steht in `SOURCES.md`.
- `strass.jpg`: 1200 × 1600, 725.987 Byte, SHA-256 `7a81802b6de85d53394a70d0897b5cc9017b1b9ce30b2fae94290a378813b713`.

## Schnittstellen

- WIT und Dokumentmodell bleiben unverändert (`OcrInfo`, `TextRegion`).
- Settings: `min_confidence` (neuer Standard 0,5), `max_regions` (neu); `engine` wird als nicht unterstützt gemeldet.
- `OcrEngine::recognize_limited` ist eine neue Methode mit Standard-Implementierung, die bestehende Engines nicht bricht.

## Datenmodell

Keine Änderung. `combined_text` trennt die Zeilen künftig durch `\n`. FTS5 indexiert den Text wie bisher; das Feld `ocr.combined_text` fließt schon heute in `photos_fts`.

## Umsetzung der Akzeptanzkriterien

Tests in `crates/host/tests/ocr_paddle_test.rs`:

| AC    | Test                                                                          |
| ----- | ----------------------------------------------------------------------------- |
| AC-1  | `test_ac1_german_text_lines`                                                  |
| AC-2  | `test_ac2_photo_with_umlaut_and_sharp_s`                                      |
| AC-3  | `test_ac3_no_text`                                                            |
| AC-4  | `test_ac4_unreadable_and_unsupported`                                         |
| AC-5  | `test_ac5_normalised_ordered_deterministic`, `test_ac5_component_matches`     |
| AC-6  | `test_ac6_min_confidence_and_max_regions`                                     |
| AC-7  | `test_ac7_no_primary_language`                                                |
| AC-8  | `test_ac8_embedded_models_and_attribution`                                    |
| AC-9  | `test_ac9_enabled_with_limits_and_large_image`                                |
| AC-10 | `test_ac10_full_text_search` (`flagme-host process`, danach `search München`) |
| AC-11 | `test_ac11_fixture_sources`                                                   |

**Anpassungen bestehender Tests** (die Kennungen bleiben):

- `no_synthetic_test.rs` AC-3/5/6: Die Erwartung „keine Engine“ gilt nur noch für `ai-tagging`. Der Warnungstext zu `engine` ändert sich auf ADR 0009, und `ocr-text` ist an.
- `ocr_test.rs`: Die Tests mit Test-Engines bleiben. Tests, die `new()` ohne Engine oder `min_confidence` 0,0 voraussetzen, werden angepasst.
- Die Beispielausgabe in `docs/pipeline_host_supervisor.md` bekommt `ocr`, falls `sample.jpg` Text enthält; sonst steht dort `ocr` mit leerem `combined_text`.

## Sicherheit

- _Strikte Regeln_ 1: nur Rust (`rten`, `image`), kein `unsafe` im Plugin, nur die sichere API `load_static_slice`.
- _Strikte Regeln_ 2: Text stammt nur aus den Modellen. Fehler ergeben `None`; was unter `min_confidence` liegt, entfällt sichtbar über die Einstellung.
- _Security_ 3: eigene Zeitgrenze von 30 s; die Zahl der Zeilen ist durch `max_regions` begrenzt.
- _Architektur/Layering_ 3: keine Abhängigkeit von `face-detection`. Die Prüfung auf abgeschnittene JPEGs wird dupliziert.

## Messwerte

Gemessen am 2026-11-27 auf macOS (Apple Silicon), Release-Build, Wasm-SIMD, über den Host (inkl. Instanziierung, Laden der Modelle, Dekodieren).

| Messung | Wert |
| :-- | --: |
| Komponente `flagme_plugin_ocr_text.wasm` (mit 12,9 MB Modellen) | 17.770.271 Byte (vorher ≈ 0,34 MB) |
| `german-text.png` (1400 × 420, 4 Zeilen) | 753 ms |
| `strass.jpg` (1200 × 1600) | 892 ms |
| 3000 × 4000, aus `strass.jpg` hochskaliert (Test AC-9) | 1,17 s |
| höchster Speicher des Host-Prozesses, `strass.jpg` | 236 MB |

Ergänzungen zur Umsetzung:
- `rten` ist unoptimiert extrem langsam. Der Workspace optimiert die `rten`-Crates deshalb auch im Profil `dev`, wie schon die Cranelift-Crates (`Cargo.toml`); ein nativer Plugin-Test sank damit von über 60 s auf 1,2 s.
- `no-synthetic-output` AC-3: Das synthetische Streifenbild (`plain_jpeg`) enthält für PaddleOCR eine Zeile. Für Plugins mit echter Engine prüft der Test deshalb, dass das Ergebnis nur von den Bildbytes abhängt und nicht vom irreführenden Dateinamen (der Kern von ADR 0002).

## Offene Punkte

- Keine.
