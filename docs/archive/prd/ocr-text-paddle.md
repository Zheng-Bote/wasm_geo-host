---
feature: ocr-text-paddle
status: archived
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
release: 0.4.0
---

# PRD — Texterkennung mit PP-OCRv5 (`ocr-text-paddle`)

## Problem

`ocr-text` hat keine Engine; seit ADR 0002 ist es abgeschaltet. Text auf Fotos (Schilder, Dokumente, Zähler, Buchcover) landet weder in der Datenbank noch in der Volltextsuche.

## Ziel

`ocr-text` erkennt Text mit den eingebetteten Modellen von PaddleOCR in `rten`, im Gast und mit Wasm-SIMD ([ADR 0009](../../adr/0009-plugin-inference-rten.md)). Erkannt werden Umlaute, ß und die übrigen Zeichen lateinischer Schrift. Das Plugin ist standardmäßig an.

**Prototyp vom 2026-11-27** (Release-Build, `wasm32-wasip2` mit SIMD, macOS, Apple Silicon):

| Bild                                                                    |     Zeit in Wasm | erkannt (Auszug)                                                    |
| :---------------------------------------------------------------------- | ---------------: | :------------------------------------------------------------------ |
| Testtext 1400 × 420 (selbst gerendert)                                  |           0,56 s | „Grüße aus München“, „Straße 12, Größe: 3,5 m“, „Öffnungszeiten: …“ |
| Ortsschild, Foto 1200 × 1600 (CC0)                                      | — (nativ 0,17 s) | „Rückholz“, „Straß“                                                 |
| Buchcover 1200 × 1600                                                   |           1,74 s | „THE RUST PROGRAMMING LANGUAGE“, „STEVE KLABNIK“                    |
| Foto 2268 × 4032 mit 55 Textbereichen (Foto des Maintainers, nur lokal) |            4,3 s | „SWM Infrastruktur GmbH & Co. KG“, „Auf Rückspann…“                 |

Zum Vergleich wurde `ocrs` (Modelle CC BY-SA 4.0) gemessen. Es erkennt keine Umlaute („Gr??e aus M?nchen“) und wurde deshalb verworfen.

## Nutzer

- **Fotografen und Betreiber:** Text auf Fotos ist durchsuchbar (`flagme-host search`, FTS5) und steht im Dokument.

## Umfang

- **Modelle** von PaddlePaddle auf Hugging Face, Apache-2.0, im ONNX-Format:
  - `PP-OCRv5_mobile_det` (Textbereiche, 4.826.518 Byte);
  - `latin_PP-OCRv5_mobile_rec` (Zeichen, 8.042.023 Byte, 836 Zeichen samt Umlauten, ß, é, è, à, ç, €).

  Sie liegen unter `plugins/ocr-text/models/` mit den zugehörigen `inference.yml`, `LICENSE` (Apache-2.0) und `SOURCE.md` (URLs, Größen, SHA-256). Eingebettet sind sie mit `include_bytes!`, zusammen ≈ 12,9 MB, also unter der Grenze von 16 MB aus ADR 0009. Das Zeichenverzeichnis wird aus der eingebetteten `inference.yml` des Erkennungsmodells gelesen.

- **Engine `PaddleOcrEngine`** (implementiert `OcrEngine`):
  1. **Dekodieren:** JPEG und PNG wie bei `face-detection`, ohne EXIF-Drehung. Abgeschnittene JPEGs werden abgelehnt.
  2. **Textbereiche finden:** die längste Seite auf 960 px bringen (ein Vielfaches von 32), normieren (ImageNet-Mittelwert und -Streuung, BGR) und die Wahrscheinlichkeitskarte auswerten:
     - Schwelle 0,3;
     - zusammenhängende Flächen als achsenparallele Rechtecke;
     - mittlere Wahrscheinlichkeit ≥ 0,6;
     - Erweiterung um den Faktor 1,5 wie im DB-Verfahren von PaddleOCR.
  3. **Zeilen zusammenführen:** Bereiche, die sich senkrecht überwiegend überdecken und waagrecht dicht beieinander liegen, werden vor dem Lesen zu einer Zeile verbunden.
  4. **Lesen:** jede Zeile auf 48 px Höhe bringen (Breite seitenverhältnistreu, mindestens 320 px) und als BGR auf −1..1 normieren. Die Ausgabe wird per CTC dekodiert (Index 0 ist leer, der letzte Index ist das Leerzeichen); die Sicherheit einer Zeile ist der Mittelwert der Zeichen-Wahrscheinlichkeiten.
  5. **Ergebnis:**
     - `TextRegion { text, confidence, bbox }` mit auf 0..1 normiertem Rahmen, in Leserichtung (Zeile, dann links nach rechts);
     - `combined_text` als Zeilen, durch `\n` getrennt;
     - `primary_language = None`, denn die Sprache wird nicht bestimmt (ADR 0002).
- **Grenze:** höchstens `max_regions` Zeilen je Foto (Standard 200, Bereich 1–1000). Es werden die sichersten gelesen; ist die Grenze erreicht, warnt das Plugin.
- **Plugin:**
  - `OcrTextPlugin::new()` nutzt `PaddleOcrEngine`; `with_engine` bleibt für Tests.
  - `min_confidence` hat den Standard **0,5** (bisher 0,0).
  - `engine` wird weiterhin als nicht unterstützt gemeldet.
- **Konfiguration:**
  - `ocr-text` ist in `config.yaml`, `config.mac-studio.yaml` und `AppConfig::default()` an;
  - dazu `plugins.ocr-text.resource_limits.timeout_ms: 30000`;
  - der Hinweis „experimental“ entfällt.
- **Testbilder** unter `crates/host/tests/fixtures/ocr/` mit `SOURCES.md`:
  - `german-text.png`: selbst gerendert mit ImageMagick, gemeinfrei (CC0);
  - `strass.jpg`: Wikimedia Commons „Ortsschild Straß (Seeg)“, CC0, Bruno413, verkleinert auf 1600 px.

  Fotos des Maintainers kommen nicht ins Repository.

- **Dokumentation:** README des Plugins, `NOTICE` (PaddleOCR, Apache-2.0), Haupt-README, arc42 (Risiko 1), CHANGELOG, Beispielausgabe in `docs/pipeline_host_supervisor.md`.

## Nicht-Ziele

- Kein gedrehter oder gekrümmter Text: Bereiche sind achsenparallel, das Bild wird nicht gerade gezogen.
- Keine Spracherkennung (`primary_language` bleibt leer).
- Keine anderen Schriften (Kyrillisch, Griechisch, CJK) und keine Modellwahl per Setting.
- Kein Halbgeviertstrich („–“) und keine anderen Zeichen außerhalb des Zeichenverzeichnisses; sie fallen weg.
- Keine EXIF-Drehung, kein HEIC oder AVIF.
- `ai-tagging` folgt als eigenes Feature.

## Akzeptanzkriterien

- **AC-1** Auf `german-text.png` liefert `ocr-text` genau 4 Zeilen, darunter `Grüße aus München` und `Straße 12, Größe: 3,5 m`; eine Zeile beginnt mit `Öffnungszeiten:`, die letzte ist `The quick brown fox jumps 2026`. `combined_text` enthält alle vier, durch Zeilenumbrüche getrennt.
- **AC-2** Auf dem Foto `strass.jpg` enthält das Ergebnis `Straß` und `Rückholz`.
- **AC-3** Ein Bild ohne Text (`Examples/sample.jpg`) liefert `Some` mit leerer Liste und leerem `combined_text`.
- **AC-4** Eine nicht lesbare Datei, ein abgeschnittenes JPEG oder ein nicht unterstütztes Format (HEIC) lassen `doc.ocr = None`. Das Plugin warnt mit Pfad und Ursache.
- **AC-5** Jede Zeile hat einen auf 0..1 normierten Rahmen und eine Sicherheit in 0..1. Die Reihenfolge ist die Leserichtung, und dasselbe Bild ergibt zweimal dasselbe Ergebnis. Die Komponente im Host liefert dieselben Zeilen wie das Plugin nativ.
- **AC-6** `min_confidence` hat den Standard 0,5 und filtert weiterhin. `max_regions` begrenzt die Zahl der gelesenen Zeilen; wird die Grenze erreicht, warnt das Plugin.
- **AC-7** `primary_language` ist immer `None`.
- **AC-8** Beide Modelle sind eingebettet; ihre SHA-256 stimmen mit `models/SOURCE.md` überein, und die Lizenzdatei (Apache-2.0) liegt daneben. `NOTICE` und die README nennen PaddleOCR, PP-OCRv5 und Apache-2.0.
- **AC-9** `ocr-text` ist in `config.yaml`, `config.mac-studio.yaml` und `AppConfig::default()` an, mit `timeout_ms: 30000` je Plugin und ohne Hinweis „experimental“. Ein Bild mit 4000 × 3000 Pixel, im Test erzeugt, läuft über den Host innerhalb dieser Grenzen.
- **AC-10** Text aus `ocr-text` ist über die Volltextsuche auffindbar: Nach `process` von `german-text.png` findet `search München` das Dokument.
- **AC-11** `crates/host/tests/fixtures/ocr/SOURCES.md` nennt für jedes Testbild Herkunft, Urheber, Lizenz, Bearbeitung und SHA-256.
