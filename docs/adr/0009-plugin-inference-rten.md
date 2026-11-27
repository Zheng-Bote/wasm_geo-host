---
status: accepted
date: 2026-11-27
supersedes: []
references:
  - docs/adr/0002-plugins-without-engine-no-output.md
  - docs/adr/0003-wasm-component-execution.md
  - docs/adr/0004-component-cache.md
---

# ADR 0009 — Echte Inferenz in Plugins mit rten und eingebetteten ONNX-Modellen

## Kontext

`face-detection`, `ocr-text` und `ai-tagging` haben seit [ADR 0002](0002-plugins-without-engine-no-output.md) nur eine Engine-Schnittstelle (`FaceDetector`, `OcrEngine`, `VisionModel`). Eine Engine haben sie nicht, darum sind sie in allen Konfigurationen abgeschaltet. ADR 0002, Entscheidung 6, verlangt für echte Engines eine eigene ADR. Zu regeln sind dort: Laufzeitsystem, Modellbezug, Lizenzen und die Einbettung in die Wasm-Komponenten aus ADR 0003. arc42 führt das Fehlen echter Engines als Risiko 1. Ohne Gesichtserkennung kann `privacy-redact` keine Gesichter schwärzen.

Der Maintainer hat die Reihenfolge festgelegt: zuerst `face-detection`, dann `ocr-text`, zuletzt `ai-tagging`.

**Prototyp am 2026-11-27** (macOS, Apple Silicon, Release-Build): [`rten`](https://github.com/robertknight/rten) 0.28 lädt ONNX-Modelle in reinem Rust. Getestet wurden zwei frei lizenzierte Gesichtsdetektoren auf öffentlichen Testbildern und auf einem 9-MP-Foto (4032 × 2268). Die Zeiten gelten je Bild, einschließlich Skalieren und Auswerten.

|                                                  | YuNet (OpenCV Zoo, MIT, 232 KB) | UltraFace RFB-320 (MIT, 1,3 MB) |
| :----------------------------------------------- | ------------------------------: | ------------------------------: |
| Modell laden (Wasm)                              |                      2,2–2,5 ms |                          2,9 ms |
| nativ, 0,2–0,5 MP                                |                        13–18 ms |                          5–6 ms |
| `wasm32-wasip2` ohne SIMD                        |                      127–131 ms |                        35–37 ms |
| `wasm32-wasip2` mit `+simd128`                   |                           85 ms |                        21–22 ms |
| 9-MP-Foto, Wasm mit SIMD                         |     119 ms (+ 82 ms Dekodieren) |      57 ms (+ 75 ms Dekodieren) |
| Gesichter: Einzelbild / Gruppenbild / Viererbild |                      1 / 52 / 4 |                      1 / 44 / 4 |
| Gesichtspunkte (Augen, Nase, Mundwinkel)         |                          ja (5) |                            nein |

Die Ergebnisse in Wasm stimmen mit den nativen überein. Höchster Speicherbedarf des ganzen `wasmtime`-Prozesses beim 9-MP-Foto: 285 MB. Die Komponente mit `rten` und JPEG-Dekoder ist ≈ 4,5 MB groß, ohne Modell.

Für die Ausführung gilt: Jede Komponente wird je Dokument frisch instanziiert (ADR 0003). Ein Modell muss also bei jedem Aufruf geladen werden; bei 2–3 ms ist das unkritisch.

## Varianten

### Laufzeit

| Variante                                                                   | Vorteile                                                                                                                                                                                                        | Nachteile                                                                                                                                                                                                                          |
| :------------------------------------------------------------------------- | :-------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | :--------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **A. `rten` im Gast**                                                      | <ul><li>Reines Rust (MIT/Apache-2.0); läuft in der Sandbox ohne neue Capability.</li><li>Lädt ONNX direkt.</li><li>`ocrs`, der Kandidat für `ocr-text`, baut darauf auf.</li><li>Im Prototyp geprüft.</li></ul> | <ul><li>CPU-only, in Wasm ohne Threads; langsamer als native Laufzeiten.</li></ul>                                                                                                                                                 |
| **B. `tract` im Gast**                                                     | <ul><li>Reines Rust, breite ONNX-Abdeckung.</li></ul>                                                                                                                                                           | <ul><li>Nicht gemessen.</li><li>Für `ocr-text` mit `ocrs` bräuchte es zusätzlich `rten`, also zwei Laufzeiten.</li></ul>                                                                                                           |
| **C. WASI-NN im Host** (`wasmtime-wasi-nn` mit OpenVINO oder ONNX Runtime) | <ul><li>Schnell, später GPU möglich.</li></ul>                                                                                                                                                                  | <ul><li>Große C++-Bibliotheken im Host.</li><li>Neue WIT-Schnittstelle.</li><li>Aufwendige Auslieferung.</li></ul>                                                                                                                 |
| **D. Fremde oder in C++ gebaute Wasm-Komponente** (z. B. OpenCV DNN)       | <ul><li>Ausgereifte Bibliotheken.</li></ul>                                                                                                                                                                     | <ul><li>Widerspricht _Strikte Regeln_ 1 („Der gesamte Produktionscode … wird in sicherem, idiomatischem Rust implementiert“); bräuchte eine Verfassungsänderung.</li><li>Laut Prototyp aus Leistungsgründen nicht nötig.</li></ul> |
| **E. Externes Programm** (z. B. Tesseract)                                 | <ul><li>Ausgereift.</li></ul>                                                                                                                                                                                   | <ul><li>Externes Programm zur Laufzeit; das Muster hat ADR 0007 gerade abgeschafft.</li></ul>                                                                                                                                      |

### Modell für `face-detection`

| Variante                          | Vorteile                                                                                                            | Nachteile                                                               |
| :-------------------------------- | :------------------------------------------------------------------------------------------------------------------ | :---------------------------------------------------------------------- |
| **A. YuNet**                      | <ul><li>Gesichtspunkte für `FaceLandmarks`.</li><li>Kleinstes Modell; fand im Gruppenbild mehr Gesichter.</li></ul> | <ul><li>Etwa doppelt so langsam wie UltraFace.</li></ul>                |
| **B. UltraFace**                  | <ul><li>Schneller.</li></ul>                                                                                        | <ul><li>Keine Gesichtspunkte.</li><li>Fand weniger Gesichter.</li></ul> |
| **C. Beide, per Setting wählbar** | <ul><li>Wahlfreiheit.</li></ul>                                                                                     | <ul><li>Doppelter Aufwand für Tests, Doku und Lizenzpflege.</li></ul>   |

### Modellauslieferung

| Variante                                 | Vorteile                                                                                                                                                                         | Nachteile                                                                                                                        |
| :--------------------------------------- | :------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | :------------------------------------------------------------------------------------------------------------------------------- |
| **A. Einbetten** (`include_bytes!`)      | <ul><li>Kein Grant, keine Datei.</li><li>Modell und Code passen immer zusammen.</li><li>Bei kleinen Modellen ohne messbare Kosten (vgl. `places.bin`, 7 MB, ADR 0005).</li></ul> | <ul><li>Große Modelle vergrößern Komponente und Git-Historie.</li><li>Unter macOS werden die Daten je Instanz kopiert.</li></ul> |
| **B. Datei über den Grant `filesystem`** | <ul><li>Modelle austauschbar, Komponente klein.</li></ul>                                                                                                                        | <ul><li>Zusätzlicher Grant.</li><li>Datei und Code können auseinanderlaufen.</li></ul>                                           |

### SIMD

Wasm-SIMD (`+simd128`) beschleunigte im Prototyp um den Faktor 1,5–1,7. wasmtime unterstützt es standardmäßig.

## Entscheidung

1. **Laufzeit `rten` im Gast (A).** Echte Engines der Plugins führen ONNX-Modelle mit `rten` in der Wasm-Komponente aus. Es gibt keine WASI-NN-Schnittstelle, keine native Inferenzbibliothek im Host und kein externes Programm. Fremde oder nicht in Rust geschriebene Komponenten (D) bleiben ausgeschlossen, solange die Verfassung sie nicht zulässt.
2. **Modelle.**
   - **Lizenz:** Ein Modell wird nur genutzt, wenn seine Gewichte unter einer freien Lizenz stehen, die kommerzielle Nutzung und Weitergabe erlaubt (z. B. MIT, Apache-2.0, BSD).
   - **Ablage:** Modell und Lizenztext liegen im Repository unter `plugins/<plugin>/models/`, zusammen mit Quelle, Version und SHA-256.
   - **Nennung:** `NOTICE` und die README des Plugins nennen Modell, Urheber und Lizenz.
3. **Auslieferung durch Einbetten (A)** für Modelle bis 16 MB. Ein größeres Modell, etwa für `ai-tagging`, regelt seine eigene Feature-ADR.
4. **`face-detection` nutzt YuNet (A)**, Version `face_detection_yunet_2023mar` aus dem OpenCV Zoo, MIT.
   - Es füllt `DetectedFace.bbox`, `confidence` und `landmarks`, die fünf Punkte.
   - Embeddings und `person_id` bleiben `None`; das Wiedererkennen von Personen ist ein eigenes, späteres Feature.
5. **Ohne Ergebnis keine erfundenen Gesichter** (ADR 0002).
   - Lässt sich ein Bild nicht lesen oder dekodieren, oder scheitert das Modell, bleibt `doc.faces = None` mit Warnung.
   - `Some` mit leerer Liste heißt: geprüft, kein Gesicht gefunden.
6. **Wasm-SIMD für alle Komponenten.**
   - Die Plugins werden für `wasm32-wasip2` mit `+simd128` gebaut.
   - Das steht in `.cargo/config.toml`, damit `cargo build`, `make components` und die Tests gleich bauen.
7. **Grenzen.**
   - Ein Plugin mit echter Engine nennt im PRD seinen Zeit- und Speicherbedarf.
   - Reichen die globalen `resource_limits` nicht, setzen die mitgelieferten Konfigurationen `plugins.<name>.resource_limits`.
   - Ein Plugin mit echter Engine ist nicht mehr experimentell (ADR 0002, Entscheidung 4) und darf standardmäßig eingeschaltet sein.
8. **Testdaten.**
   - Tests mit echten Modellen nutzen nur Bilder mit geklärter Lizenz: gemeinfrei, CC0 oder freie Lizenz mit Namensnennung.
   - Die Quelle steht neben dem Bild.
   - Fotos des Maintainers kommen nicht ins Repository.

`ocr-text` (Kandidat `ocrs`, ebenfalls auf `rten`) und `ai-tagging` folgen als eigene Features. Ihre Modellwahl und Lizenzprüfung gehören in deren PRD, ihre Abweichungen von Punkt 3 in eine eigene ADR.

## Konsequenzen

**Positiv**

- `face-detection` liefert echte Gesichter mit Gesichtspunkten. Damit kann `privacy-redact` erstmals Gesichter schwärzen (arc42, Risiko 1).
- Alles bleibt in Rust und in der Sandbox: keine neue Capability, kein externes Programm, keine Verfassungsänderung.
- Eine Laufzeit für alle drei Plugins.
- SIMD beschleunigt auch andere Komponenten, etwa die Bildverarbeitung in `thumbnail` und `duplicate`.

**Negativ**

- Gerechnet wird nur auf der CPU in einem Thread: etwa 0,2 s je 9-MP-Foto für `face-detection` (Dekodieren und Erkennen). Große Bestände brauchen entsprechend Zeit; `watch --workers` verteilt die Arbeit auf mehrere Instanzen.
- `face-detection` wird ≈ 4,5 MB groß statt ≈ 0,3 MB.
- YuNet arbeitet mit einer Eingabe von 640 × 640. Sehr kleine Gesichter in großen Gruppenfotos können fehlen; Kacheln oder höhere Auflösungen wären eine spätere Erweiterung.
- Mit `+simd128` brauchen die Komponenten eine Wasm-Laufzeit mit SIMD. wasmtime erfüllt das; die WASI-CLI-Binaries für fremde Laufzeiten ebenfalls nur mit SIMD.

**Verfassung**

- Keine Regel ändert sich. Variante D hätte eine Änderung von _Strikte Regeln_ 1 gebraucht.
- ADR 0002 bleibt gültig: Diese ADR ist die dort in Entscheidung 6 verlangte eigene ADR.
