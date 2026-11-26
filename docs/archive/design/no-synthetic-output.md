---
feature: no-synthetic-output
status: archived
archive-after:
touches:
  - plugins/metadata/**
  - plugins/ocr-text/**
  - plugins/ai-tagging/**
  - plugins/face-detection/**
  - plugins/privacy-redact/**
  - crates/sdk/**
  - crates/host/tests/**
  - crates/host/src/**
  - config*.yaml
  - README.md
  - CHANGELOG.md
---

# Tech Spec — Keine synthetischen Plugin-Ergebnisse (`no-synthetic-output`)

Setzt das PRD `docs/prd/no-synthetic-output.md` um. Die Verfassung hat Vorrang; diese Tech Spec darf sie präzisieren, nie lockern. Grundlage ist [ADR 0002](../../adr/0002-plugins-without-engine-no-output.md).

## Architektur

```text
Plugin::process(doc, ctx)
  ├─ engine = self.engine (Option<Box<dyn Engine>>)
  │    └─ None ──► warn "no <kind> engine available (ADR 0002)"; Feld bleibt None
  ├─ bytes = ctx.capabilities.fs_read(doc.file.path)
  │    └─ Err ──► warn "cannot read …"; Feld bleibt None
  └─ engine.<run>(&bytes)  ──► Ok(result) ──► Feld = Some(result)
                           └─ Err(reason) ──► warn; Feld bleibt None
```

Die Engine erhält ausschließlich die Bildbytes, nie Pfad oder Dateiname (AC-2). Kein Plugin greift direkt auf `std::fs` oder externe Programme zu.

## Schnittstellen

### Engine-Traits (ersetzen die bisherigen Traits)

```rust
// plugins/ocr-text
pub trait OcrEngine: Send + Sync {
    fn name(&self) -> &str;
    fn recognize(&self, image: &[u8]) -> Result<OcrInfo, String>;
}

// plugins/ai-tagging
pub trait VisionModel: Send + Sync {
    fn model_name(&self) -> &str;
    fn analyze(&self, image: &[u8], embedding_dim: usize) -> Result<AiInfo, String>;
}

// plugins/face-detection
pub trait FaceDetector: Send + Sync {
    fn model_name(&self) -> &str;
    fn detect(&self, image: &[u8]) -> Result<Vec<DetectedFace>, String>;
}
```

Konstruktoren: `new()` ohne Engine; `with_engine(Box<dyn OcrEngine>)`, `with_model(Box<dyn VisionModel>)`, `with_detector(Box<dyn FaceDetector>)` mit Engine. `FaceDetectionPlugin::with_tier` und `ModelTier` entfallen.

Entfernt werden: `DefaultOcrEngine`, `BuiltinEngine`; `ImageFeatures`, `MobileNetModel`, `ClipModel`, `OpenClipModel`, `BuiltinModel`; `LightweightFaceDetector`, `HeavyFaceDetector`, Signatur-Tabelle, Hautton-Detektor (`djpeg`), `generate_face_embedding`, `landmarks_from_bbox`; die `Device`-Enums.

### Nachbearbeitung im Plugin

- `ocr-text`: `filter_by_confidence(min_confidence)` wie bisher.
- `face-detection`: Gesichter unter `min_confidence` werden verworfen; `detection_model = engine.model_name()`.
- `ai-tagging`: Hat das Embedding der Engine nicht die Länge `embedding_dim`, wird es mit Warnung verworfen (`embedding = None`, Tags bleiben). Sonst wird es L2-normiert; ein Nullvektor wird verworfen.

### Settings (SDK)

Neue Methode in `crates/sdk/src/settings.rs`:

```rust
/// Consumes `key`; any value is reported as "not supported yet" and ignored.
pub fn not_supported(&mut self, key: &str, aliases: &[&str], reason: &str)
```

Warnung: `settings[<plugin>]: '<value>' for '<key>' is not supported yet (<reason>), ignored`.

| Plugin | Ignoriert (`not_supported`) | Wirksam |
| :--- | :--- | :--- |
| `ocr-text` | `engine` | `min_confidence` |
| `ai-tagging` | `model`, `model_tier`, `device` | `embedding_dim` |
| `face-detection` | `tier`, `device` | `min_confidence` |

### `metadata`

- Schritt 5 (synthetischer Fallback bei < 128 Byte) entfällt ersatzlos.
- Bei `fs_read`-Fehler: Warnung `metadata: cannot read '<path>': <err>; metadata left empty`, `doc` bleibt unverändert.

### `privacy-redact`

Bei `redact_faces = true` und `doc.faces = None`: Warnung `privacy-redact: no face detection result for '<path>'; faces NOT redacted`. Keine Regel `redacted_…`. `Some` mit leerer Liste bleibt still.

### Konfiguration

`config.yaml`, `config.docker.yaml`, `config.mac-studio.yaml` und `AppConfig::default()` (`crates/host/src/config.rs`): `ocr-text`, `ai-tagging`, `face-detection` erhalten `enabled: false` mit Kommentar „experimental, no engine (ADR 0002)“. Engine-Settings werden aus den Profilen entfernt.

## Datenmodell

Keine Änderung. `doc.ocr`, `doc.ai`, `doc.faces` bleiben `Option`; `None` heißt „nicht bestimmt“.

## Umsetzung der Akzeptanzkriterien

| AC | Umsetzung | Test (`crates/host/tests/no_synthetic_test.rs`) |
|---|---|---|
| AC-1 | `metadata` ohne Fallback | `test_ac1_metadata_unreadable_and_tiny_files` |
| AC-2 | Engines erhalten nur Bytes; keine Namensregeln | `test_ac2_output_independent_of_file_name` |
| AC-3 | Engines optional, keine eingebauten | `test_ac3_no_engine_no_result` |
| AC-4 | Pipeline über Test-Engines, Embedding-Prüfung | `test_ac4_engine_results_passed_through`, `test_ac4_read_and_engine_errors`, `test_ac4_embedding_normalised_and_checked` |
| AC-5 | `SettingsReader::not_supported` | `test_ac5_engine_settings_not_supported` |
| AC-6 | `config*.yaml`, `AppConfig::default()` | `test_ac6_shipped_configs_disable_experimental_plugins` |
| AC-7 | `privacy-redact` | `test_ac7_privacy_redact_warns_without_face_result` |

Bestehende Tests, die auf erfundenem Verhalten beruhen (`ocr_test`, `face_test`, `fts_test`, `ac_test`, `vector_test`, `watcher_test`, `settings_test`), nutzen Test-Engines bzw. echte Testdateien. Kriterien bestehender PRDs, die nur mit einer echten Engine erfüllbar sind, werden dort als entfallen markiert oder auf die Engine-Schnittstelle präzisiert.

## Sicherheit

- Entfernt den Prozessaufruf `djpeg` und die direkten `std::fs`-Lesezugriffe der Engines (Verfassung Security 1–2, Teil von #25).
- `privacy-redact` macht fehlende Gesichtserkennung sichtbar, statt still nichts zu schwärzen.

## Offene Punkte

- Wahl des Laufzeitsystems und der Modelle für echte Engines (eigenes ADR, abgestimmt mit #30).
