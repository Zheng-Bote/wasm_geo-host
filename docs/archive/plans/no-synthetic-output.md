---
feature: no-synthetic-output
status: archived
archive-after:
---

# Plan — Keine synthetischen Plugin-Ergebnisse (`no-synthetic-output`)

Reihenfolge der Umsetzung. Jede Task nennt das Akzeptanzkriterium, auf das sie einzahlt.

## Tasks

- [x] AC-1 `metadata`: synthetischen Fallback und Ersatz-Bytes entfernen; Lesefehler warnen.
- [x] AC-5 SDK: `SettingsReader::not_supported` ergänzen.
- [x] AC-3 `ocr-text`: `DefaultOcrEngine` entfernen, `OcrEngine` auf Bytes umstellen, Engine optional.
- [x] AC-3 `ai-tagging`: Pseudo-Modelle und `ImageFeatures` entfernen, `VisionModel::analyze` auf Bytes, Modell optional.
- [x] AC-3 `face-detection`: Detektoren, Signatur-Tabelle, Hautton-Detektor und Byte-Embeddings entfernen, `FaceDetector::detect` auf Bytes, Detektor optional.
- [x] AC-4 Fehlerpfade (Lesen, Engine) und Embedding-Prüfung in `ai-tagging`.
- [x] AC-5 Engine-Settings über `not_supported` ignorieren.
- [x] AC-6 `config*.yaml`: experimentelle Plugins deaktivieren.
- [x] AC-7 `privacy-redact`: Warnung bei fehlendem Gesichtserkennungs-Ergebnis.
- [x] AC-2 Tests je Kriterium in `crates/host/tests/no_synthetic_test.rs`; bestehende Tests auf Test-Engines umstellen.
- [x] AC-3 READMEs der vier Plugins, betroffene PRDs (`face-detection`, `ocr-text`, `vector-search-embeddings`) und CHANGELOG anpassen.
