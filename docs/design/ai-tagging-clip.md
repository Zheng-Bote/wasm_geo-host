---
feature: ai-tagging-clip
status: accepted
archive-after:
touches:
  - plugins/ai-tagging/**
  - tools/clip-labels/**
  - Cargo.toml
  - Cargo.lock
  - Makefile
  - .gitignore
  - crates/host/tests/**
  - crates/host/src/config.rs
  - config.yaml
  - config.mac-studio.yaml
  - NOTICE
  - README.md
  - docs/build-instructions.md
  - docs/deployment-guide.md
  - docs/arc42.md
  - docs/pipeline_host_supervisor.md
  - CHANGELOG.md
---

# Tech Spec — Schlagwörter und Embeddings mit CLIP (`ai-tagging-clip`)

Setzt das PRD `docs/prd/ai-tagging-clip.md` um. Die Verfassung hat Vorrang; diese Tech Spec darf sie präzisieren, nie lockern. Grundlage sind [ADR 0010](../adr/0010-ai-tagging-clip.md) und [ADR 0009](../adr/0009-plugin-inference-rten.md).

## Architektur

**Modelldateien** (`make models`, Quelle `https://huggingface.co/Xenova/clip-vit-base-patch32/resolve/6b7c523703ae26e39be4b03774966f8571c7f523/<pfad>`):

| Ziel in `models/`               | Quelle                                                       |       Byte | SHA-256                                                            |
| :------------------------------ | :----------------------------------------------------------- | ---------: | :----------------------------------------------------------------- |
| `clip-vit-b32-vision-int8.onnx` | `onnx/vision_model_int8.onnx`                                | 88.648.877 | `0ab0c1b3ace708e539633af1744d5a95247fe4e14d3e08ff197ef82a6cb9bd93` |
| `clip-vit-b32-text-int8.onnx`   | `onnx/text_model_int8.onnx`                                  | 64.070.791 | `18845f2ccc35223bb7fec403383a131154b11ac0918df25cf51986df5efd3a21` |
| `clip-vit-b32-tokenizer.json`   | `tokenizer.json`                                             |  2.224.119 | `f7f3b7af117d467b58374797691a6438d3e6b9e9cef800dfd5dced7f697a90cd` |
| `CLIP-LICENSE`                  | `https://raw.githubusercontent.com/openai/CLIP/main/LICENSE` |          — | wird nicht geprüft (Text)                                          |

**`Makefile`**

- `models`:
  - für jede Datei: `curl -fsSL` nach `<ziel>.part`, danach `shasum -a 256 -c` gegen `models/SHA256SUMS`;
  - gelingt die Prüfung, wird die Datei umbenannt, sonst gelöscht und mit Fehler abgebrochen;
  - liegt eine Datei mit passendem SHA-256 schon vor, wird nichts geladen.
- `models/SHA256SUMS` ist die einzige committete Datei unter `models/`; `.gitignore` schließt `models/*` außer `!models/SHA256SUMS` aus.
- `labels`: `cargo run --release -p clip-labels -- --labels plugins/ai-tagging/labels/labels.txt --model models/clip-vit-b32-text-int8.onnx --tokenizer models/clip-vit-b32-tokenizer.json --output plugins/ai-tagging/labels/labels.bin`.
- `test: models`; `dist` kopiert `models/clip-vit-b32-vision-int8.onnx` und `models/CLIP-LICENSE` nach `<paket>/models/` des Gesamtpakets und des Plattformpakets.

**`plugins/ai-tagging/labels/`**

- `labels.txt`: ein Begriff je Zeile, `#` leitet einen Kommentar ein, Leerzeilen werden ignoriert, die Reihenfolge ist die Ausgabereihenfolge der Vektoren.
- `labels.bin`, Formatversion 1, Little Endian:

  | Feld                     | Inhalt                       |
  | :----------------------- | :--------------------------- |
  | Magic                    | `"FCLB"`                     |
  | Version                  | u16 = 1, dann u16 reserviert |
  | Anzahl                   | u32                          |
  | Dimension                | u32 = 512                    |
  | SHA-256 von `labels.txt` | 32 Byte                      |
  | SHA-256 des Textmodells  | 32 Byte                      |
  | Vektoren                 | Anzahl × 512 × f32           |
  | Begriffe                 | je u16-Länge plus UTF-8      |

**`tools/clip-labels`** (neu, `publish = false`)

- Abhängigkeiten: `rten` (`onnx_format`), `rten-tensor`, `tokenizers = { version = "0.21", default-features = false, features = ["fancy-regex"] }` (reines Rust), `sha2`.
- Ablauf:
  1. Begriffe lesen; doppelte Begriffe sind ein Fehler.
  2. Je Begriff `tokenizer.encode("a photo of {label}", true)`; die IDs als `i32`-Tensor `[1, n]` an `input_ids`.
  3. Ausgabe `text_embeds` lesen, L2-normieren und schreiben.

  Die Ausgabe entsteht als `<file>.tmp` und wird erst am Ende umbenannt.

**`plugins/ai-tagging/src/clip.rs`** (neu)

- Konstanten:
  - `MODEL_SHA256` (s. o.), `MODEL_NAME = "clip-vit-b32"`, `DEFAULT_MODEL_PATH = "models/clip-vit-b32-vision-int8.onnx"`;
  - `IMAGE_SIZE = 224`, CLIP-Mittelwert `0.48145466, 0.4578275, 0.40821073`, Streuung `0.26862954, 0.26130258, 0.27577711`;
  - `LOGIT_SCALE = 100.0`, `EMBEDDING_DIM = 512`.
- `pub static LABELS: &[u8] = include_bytes!("../labels/labels.bin");`, dazu `Labels::parse(bytes)`, das Magic, Version 1, Dimension 512 und die Längen prüft.
- `ClipModel::from_model_bytes(bytes: Vec<u8>) -> Result<Self, String>`:
  - prüft den SHA-256 gegen `MODEL_SHA256` (Fehler „model file … has SHA-256 …, expected …; run `make models`“);
  - lädt mit `Model::load(bytes)`;
  - sucht die Knoten `pixel_values` und `image_embeds`;
  - lädt `Labels::parse(LABELS)`.
- `impl VisionModel for ClipModel`, `analyze(image, embedding_dim)`:
  1. Dekodieren wie in den anderen Plugins (JPEG/PNG, EOI-Prüfung; die Prüffunktion wird kopiert, _Architektur/Layering_ 3).
  2. Die kürzere Seite auf 224 skalieren (`CatmullRom`), mittig 224 × 224 zuschneiden, RGB `(x/255 − mean)/std`.
  3. `image_embeds` → L2-normieren → `embedding`.
  4. `logits[i] = 100 · dot(embedding, label_i)`; Softmax.
  5. `tags`: alle mit p ≥ `min_confidence`, absteigend, höchstens `max_tags`, `AiTag { label, confidence: p, model: "clip-vit-b32" }`.
  6. `description = ""`.

  `max_tags` und `min_confidence` übergibt das Plugin über eine neue Methode `ClipModel::analyze_with(image, &TagParams)`. `VisionModel::analyze` nutzt die Standardwerte.

**`plugins/ai-tagging/src/lib.rs`**

- `AiTaggingSettings`:
  - `model_path` (`r.path`, Standard `DEFAULT_MODEL_PATH`);
  - `max_tags` (1–20, Standard 5);
  - `min_confidence` (0–1, Standard 0,10);
  - `embedding_dim` bleibt lesbar, wird aber auf 512 festgelegt: ein anderer Wert ergibt die Warnung „embedding_dim is fixed to 512 by the CLIP model“.
  - `model`/`model_tier`/`device`: Grund „only the built-in CLIP model is available, see ADR 0010“.
- `AiTaggingPlugin::new()` hat keine eingesetzte Engine.
  - `process` liest `model_path` per `ctx.capabilities.fs_read` und baut daraus `ClipModel`.
  - Scheitert das, gibt es die Warnung `ai-tagging: CLIP model unavailable at '<path>': <ursache> (run \`make models\`)`, und `doc.ai`bleibt`None`.
  - `with_model` bleibt für Tests: Eine eingesetzte Engine hat Vorrang, und `model_path` wird dann nicht gelesen.
- Der Rest bleibt: Bild lesen, `normalize_embedding`, Warnungen.

**Konfiguration**

- `config.yaml`, `config.mac-studio.yaml` und `AppConfig::default()`: `ai-tagging` mit `enabled: true` und `resource_limits` `max_memory_bytes: 536870912`, `timeout_ms: 30000`.
- `config.mac-studio.yaml`: `settings.embedding_dim: '512'` bleibt gültig.

**Tests**

- `common::clip_model_path()` gibt `<repo>/models/clip-vit-b32-vision-int8.onnx` zurück. Fehlt die Datei, panikt es mit „CLIP model missing: run `make models`“.
- Komponententests setzen `model_path` absolut, weil das Arbeitsverzeichnis der Tests `crates/host` ist.

## Schnittstellen

- WIT und Dokumentmodell bleiben unverändert (`AiInfo { tags, description, embedding }`).
- Settings: `model_path`, `max_tags`, `min_confidence` sind neu; `embedding_dim` ist auf 512 festgelegt.
- CLI: `flagme-host similar` bleibt unverändert und nutzt jetzt echte Embeddings.

## Datenmodell

Keine Schemaänderung. `photo_embeddings` speichert das 512-dimensionale Embedding mit `model = "clip-vit-b32"`; die Tags gehen wie bisher in `ai_tags` und in den Volltextindex.

## Umsetzung der Akzeptanzkriterien

Tests in `crates/host/tests/clip_test.rs`, das Werkzeug in `tools/clip-labels/tests/`:

| AC    | Test                                                                                                                   |
| ----- | ---------------------------------------------------------------------------------------------------------------------- |
| AC-1  | `test_ac1_space_photos`                                                                                                |
| AC-2  | `test_ac2_similarity`, `test_ac2_similar_command` (`process` für vier Bilder, dann `similar`)                          |
| AC-3  | `test_ac3_tag_rules`                                                                                                   |
| AC-4  | `test_ac4_missing_or_wrong_model_and_bad_images`                                                                       |
| AC-5  | `test_ac5_models_target` (Makefile-Inhalt, `.gitignore`, `SHA256SUMS`; kein Download im Test)                          |
| AC-6  | `test_ac6_labels_without_personal_attributes`                                                                          |
| AC-7  | `tools/clip-labels` `test_ac7_deterministic` sowie `clip_test.rs` `test_ac7_labels_bin_matches_txt`                    |
| AC-8  | `test_ac8_component_matches_within_limits`                                                                             |
| AC-9  | `test_ac9_enabled_with_limits`                                                                                         |
| AC-10 | `test_ac10_dist_contains_model` (Makefile-Inhalt) sowie ein Smoke-Test des Pakets von Hand, Ergebnis unter _Messwerte_ |
| AC-11 | `test_ac11_attribution`                                                                                                |

**Anpassungen bestehender Tests** (die Kennungen bleiben):

- `no_synthetic_test.rs` AC-3/5/6: Kein Plugin ist mehr ohne Engine. AC-3 prüft für alle drei die Unabhängigkeit vom Dateinamen (bei `ai-tagging` mit Modell), dazu den Warnungstext und dass ai-tagging an ist.
- `ac_test.rs`, `examples_test.rs` und die Beispielausgabe: `ai` erscheint jetzt (Embedding und Tags von `sample.jpg`).
- `vector_test.rs`, `settings_test.rs` (Standardwerte, Warnungstexte): passen sich an.

## Sicherheit

- _Strikte Regeln_ 1: nur Rust; `tokenizers` nur im Werkzeug, nicht im Plugin. Kein `unsafe`; `Model::load` statt `load_mmap`.
- _Strikte Regeln_ 2: Schlagwörter stammen nur aus der festen Liste und dem Modell, `description` bleibt leer, Fehler ergeben `None`.
- _Security_ 1–3: Der Grant `filesystem` bestand schon. Das Modell wird vor dem Laden per SHA-256 geprüft, sodass eine untergeschobene Datei abgelehnt wird. Eigene Grenzen gelten (512 MB, 30 s).
- ADR 0010, Punkt 5: Der Test für AC-6 prüft `labels.txt` gegen eine Sperrliste.

## Messwerte

Gemessen am 2026-11-27 auf macOS (Apple Silicon), Release-Build, Wasm-SIMD.

| Messung | Wert |
| :-- | --: |
| `flagme_plugin_ai_tagging.wasm` (ohne Modell, mit `labels.bin`) | 5.413.434 Byte (vorher ≈ 0,31 MB) |
| `labels.bin` (278 Begriffe × 512) | 572.478 Byte |
| `ai-tagging` über den Host je Foto (Lesen von 89 MB, SHA-256, Laden, Lauf), `apollo11.jpg` / `strass.jpg` | 866 ms / 819 ms |
| 4000 × 3000 über den Host (Test AC-8, Debug-Host) | 1,36 s |
| höchster Speicher des Host-Prozesses | 320 MB |
| Gast-Speicher, 9-MP-Foto (Prototyp, `max-memory-size`) | 160–192 MB |
| Kosinus Wasm-Komponente gegen nativ (int8-Kerne) | 0,9968 |
| Plattformpaket `darwin-arm64` vorher / nachher | 25,1 MB / 122,5 MB |
| Gesamtpaket vorher / nachher | 31,7 MB / 145,3 MB |

Smoke-Test des entpackten Plattformpakets (AC-10): `process` auf `apollo11.jpg` liefert ein Embedding mit 512 Werten und die Schlagwörter `a space suit` 0,27, `the moon` 0,14 und `outer space` 0,11; `face-detection` und `privacy-redact` schwärzen 3 Gesichter. Der erste Lauf dauerte 5,3 s, weil alle Komponenten erstmals kompiliert werden.

Ergänzungen zur Umsetzung:
- Der Ausschluss von `models/` steht in `models/.gitignore`, nicht in der Wurzel-`.gitignore`; die Wirkung ist dieselbe.
- Mit 278 Begriffen verteilt sich die Softmax breiter als im 24-Begriffe-Prototyp (bestes Schlagwort 0,2–0,5). PRD AC-1 und AC-8 wurden daran angepasst; „astronauts“ ist als Beruf ausgeschlossen (ADR 0010, Punkt 5).
- Der Paketbau für den Smoke-Test lief versehentlich unter der schon veröffentlichten Version 0.3.0 und überschrieb die lokalen Pakete in `dist/packages`. Sie wurden aus der geprüften Download-Kopie des GitHub-Releases wiederhergestellt.

## Offene Punkte

- Keine.
