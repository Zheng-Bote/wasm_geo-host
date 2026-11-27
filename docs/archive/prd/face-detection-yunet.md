---
feature: face-detection-yunet
status: archived
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
release: 0.4.0
---

# PRD — Gesichtserkennung mit YuNet (`face-detection-yunet`)

## Problem

`face-detection` hat keine Engine; seit ADR 0002 ist es abgeschaltet und liefert nichts. `privacy-redact` kann darum keine Gesichter schwärzen und warnt bei jedem Foto „no face detection result … faces NOT redacted“. arc42 führt das als Risiko 1.

## Ziel

`face-detection` erkennt Gesichter mit dem eingebetteten Modell YuNet in `rten`, im Gast und mit Wasm-SIMD ([ADR 0009](../../adr/0009-plugin-inference-rten.md)).

- Es liefert für jedes Gesicht einen normierten Rahmen, eine Wahrscheinlichkeit und fünf Gesichtspunkte.
- `privacy-redact` schwärzt damit echte Gesichter.
- Das Plugin ist standardmäßig an.
- Gesichter werden nur erkannt, nicht Personen zugeordnet.

## Nutzer

- **Betreiber mit Datenschutzbedarf:** Gesichter in veröffentlichten Kopien werden geschwärzt.
- **Fotografen:** In der Datenbank steht, auf welchen Fotos Gesichter sind und wo.

## Umfang

- **Modell:** `face_detection_yunet_2023mar.onnx` aus dem OpenCV Zoo (MIT, 232.589 Byte).
  - Ablage: `plugins/face-detection/models/` mit `LICENSE` und `SOURCE.md` (URL, Version, SHA-256).
  - Eingebettet mit `include_bytes!`.
- **Engine `YuNetDetector`** (implementiert `FaceDetector`):
  1. Dekodiert JPEG und PNG mit `image`. Die EXIF-Drehung wird nicht angewendet, wie in `privacy-redact`, damit die Rahmen zu den Pixeln passen.
  2. Skaliert seitenverhältnistreu auf 640 × 640, mit Rand, und gibt das Bild als BGR in 0..255 an das Modell.
  3. Wertet die Ausgaben der Strides 8, 16 und 32 aus. Die Wahrscheinlichkeit ist `sqrt(cls · obj)`.
  4. Verwirft Treffer unter 0,6 und entfernt Doppelte per Non-Maximum-Suppression (IoU 0,3).
  5. Normiert Rahmen und Gesichtspunkte auf 0..1 bezogen auf das Originalbild.
  6. Vergibt IDs `face-1` … `face-n` in Leserichtung (nach `y`, dann `x` des Rahmens).
- **Plugin:**
  - `FaceDetectionPlugin::new()` nutzt `YuNetDetector`; `with_detector` bleibt für Tests.
  - `detection_model = "yunet-2023mar"`.
  - `min_confidence` filtert wie bisher.
- **Settings:** `model`, `tier` und `device` werden weiterhin mit „not supported“ gemeldet; es gibt nur YuNet.
- **Wasm-SIMD:** `.cargo/config.toml` setzt `target-feature=+simd128` für `wasm32-wasip2` (ADR 0009, Punkt 6).
- **Konfiguration:** `face-detection` ist in `config.yaml` und `config.mac-studio.yaml` an. Der Hinweis „experimental“ entfällt.
- **Testbilder:** gemeinfreie NASA-Fotos von Wikimedia Commons, verkleinert auf 1600 px:
  - „Apollo 11 Crew“ (3 Gesichter);
  - „Expedition 64 crew portrait“ (7 Gesichter).

  Sie liegen unter `crates/host/tests/fixtures/faces/` mit `SOURCES.md`. Fotos des Maintainers kommen nicht ins Repository.

- **Dokumentation:** README des Plugins (nicht mehr experimentell; Modell, Lizenz, Grenzen), `NOTICE` (YuNet, MIT), Haupt-README, arc42 (Risiko 1), CHANGELOG.

## Nicht-Ziele

- Keine Gesichts-Embeddings, kein `person_id`, keine Wiedererkennung von Personen.
- Keine Kacheln oder höhere Auflösung für sehr kleine Gesichter.
- Keine Anwendung der EXIF-Drehung (gilt hier wie in `privacy-redact`).
- Kein HEIC oder AVIF (kein Dekoder vorhanden).
- Keine Modellwahl per Setting und kein zweites Modell.
- `ocr-text` und `ai-tagging` folgen als eigene Features.

## Akzeptanzkriterien

- **AC-1** Auf dem Testbild „Apollo 11 Crew“ liefert `face-detection` `Some` mit genau 3 Gesichtern, auf „Expedition 64 crew portrait“ mit genau 7. Jedes Gesicht hat eine Wahrscheinlichkeit ≥ 0,9, einen Rahmen innerhalb von 0..1 und fünf Gesichtspunkte innerhalb seines Rahmens.
- **AC-2** Auf einem Bild ohne Gesicht (`Examples/sample.jpg`) liefert das Plugin `Some` mit leerer Liste und `detection_model` `yunet-2023mar`.
- **AC-3** Eine nicht lesbare Datei, ein abgeschnittenes JPEG oder ein nicht unterstütztes Format (HEIC) lassen `doc.faces = None`. Das Plugin warnt mit Pfad und Ursache.
- **AC-4** IDs und Reihenfolge sind deterministisch (`face-1` … in Leserichtung), und zweimal dasselbe Bild ergibt dasselbe Ergebnis. `min_confidence` filtert weiterhin.
- **AC-5** Über den Host (Komponenten `face-detection` und `privacy-redact`) wird das Apollo-11-Bild mit 3 geschwärzten Gesichtern als neue Datei geschrieben (`faces_redacted_count = 3`). Die Pixel innerhalb der Rahmen unterscheiden sich vom Original, die Pixel weit außerhalb nicht.
- **AC-6** Das Modell ist eingebettet. Sein SHA-256 stimmt mit `SOURCE.md` überein, die Lizenzdatei liegt daneben, und `NOTICE` sowie die README nennen YuNet, OpenCV Zoo und MIT.
- **AC-7** Ein Bild von 4000 × 3000 Pixel, im Test erzeugt, läuft über den Host innerhalb der Standardgrenzen (256 MB, 10 s). Das Tech Spec nennt die im Release-Build gemessenen Zeiten.
- **AC-8** `face-detection` ist in `config.yaml` und `config.mac-studio.yaml` an, ohne Hinweis „experimental“. Ein `process`-Lauf mit `config.yaml` auf einem Bild mit Gesichtern erzeugt keine Warnung „no face detection result“ mehr.
- **AC-9** `.cargo/config.toml` setzt `+simd128` für `wasm32-wasip2`, und alle Komponenten exportieren weiter die Welt `media-plugin`.
- **AC-10** `crates/host/tests/fixtures/faces/SOURCES.md` nennt für jedes Testbild Quelle (URL), Urheber und Lizenz (Public Domain) sowie die Bearbeitung (verkleinert auf 1600 px).
