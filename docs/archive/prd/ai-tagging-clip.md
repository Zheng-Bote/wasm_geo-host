---
feature: ai-tagging-clip
status: archived
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
release: 0.4.0
---

# PRD — Schlagwörter und Embeddings mit CLIP (`ai-tagging-clip`)

## Problem

`ai-tagging` hat keine Engine (ADR 0002). Fotos bekommen deshalb keine Schlagwörter, und `flagme-host similar` findet keine ähnlichen Fotos, weil kein Embedding existiert.

## Ziel

`ai-tagging` berechnet mit dem Bildteil von CLIP ViT-B/32 (int8, MIT) in `rten` ein Embedding je Foto und vergibt Schlagwörter aus einer festen Begriffsliste ([ADR 0010](../../adr/0010-ai-tagging-clip.md), auf Grundlage von [ADR 0009](../../adr/0009-plugin-inference-rten.md)).

- Das Modell kommt als Datei über `make models`.
- Das Plugin ist standardmäßig an, mit eigenen Grenzen.
- Ohne Modelldatei liefert es nichts und warnt.

**Prototyp vom 2026-11-27** (Details in ADR 0010):

- 0,37–0,43 s je Foto in Wasm mit SIMD.
- Der Gast braucht für ein 9-MP-Foto zwischen 160 und 192 MB Speicher (läuft mit `max-memory-size` 192 MB, nicht mit 160 MB).
- Schlagwörter: „astronauts“ 0,91 und 0,95 für die NASA-Fotos, „a street sign“ 0,61, „a book cover“ 0,84.
- Kosinus der beiden Astronautenfotos 0,78, alle anderen Paare ≤ 0,52.

## Nutzer

- **Fotografen und Betreiber:** Suche nach Begriffen (Volltext über `ai.tags`) und Ähnlichkeitssuche (`flagme-host similar <id>`).

## Umfang

- **`make models`** lädt von `https://huggingface.co/Xenova/clip-vit-base-patch32/resolve/6b7c523703ae26e39be4b03774966f8571c7f523/` nach `models/`, das in `.gitignore` steht:
  - `onnx/vision_model_int8.onnx` als `models/clip-vit-b32-vision-int8.onnx`;
  - `onnx/text_model_int8.onnx` als `models/clip-vit-b32-text-int8.onnx`;
  - `tokenizer.json` als `models/clip-vit-b32-tokenizer.json`;
  - dazu die MIT-Lizenz von OpenAI CLIP als `models/CLIP-LICENSE`.

  Jede Datei wird gegen ihren fest eingetragenen SHA-256 geprüft; bei Abweichung bricht der Lauf ab und löscht die Datei.

- **Begriffsliste** `plugins/ai-tagging/labels/labels.txt`: etwa 200–300 englische Begriffe für Fotosammlungen, ein Begriff je Zeile, Kommentare mit `#`.
  - Themen: Landschaft, Natur, Wasser, Wetter, Jahreszeiten, Stadt, Gebäude, Verkehr, Tiere, Pflanzen, Essen, Innenräume, Gegenstände, Dokumente und Schilder, Veranstaltungen, Sport, Kunst.
  - Für Personen nur neutrale Begriffe („people“, „a group of people“, „a portrait“, „a crowd“). Ausgeschlossen ist zum Beispiel „a child“, weil es ein Altersmerkmal ist.
  - Keine Begriffe zu Geschlecht, Alter, Herkunft, Hautfarbe, Religion, Aussehen, Gesundheit oder Beruf (ADR 0010, Punkt 5).
- **Werkzeug `tools/clip-labels`** (Rust, nativ, Workspace-Mitglied):
  - liest `labels.txt`, kodiert jeden Begriff als „a photo of {label}“ mit dem Tokenizer (Crate `tokenizers`) und rechnet ihn mit dem Textteil von CLIP in `rten` in einen normierten 512-dimensionalen Vektor um;
  - schreibt `plugins/ai-tagging/labels/labels.bin` (Format mit Kopf: Magic, Version, Anzahl, Dimension, SHA-256 von `labels.txt` und vom Textmodell; dann die Vektoren als f32 und die Begriffe);
  - gleiche Eingaben ergeben byte-gleiche Ausgaben;
  - `make labels` ruft das Werkzeug auf; `labels.bin` wird committet (≈ 0,5 MB).
- **Engine `ClipModel`** (implementiert `VisionModel`):
  1. Dekodieren wie bei den anderen Plugins: JPEG und PNG, abgeschnittene JPEGs werden abgelehnt.
  2. Die kürzere Seite auf 224 px skalieren (Catmull-Rom), mittig auf 224 × 224 zuschneiden und normieren (CLIP-Mittelwert und -Streuung, RGB).
  3. Der Bildteil liefert `image_embeds`; das Ergebnis wird L2-normiert.
  4. Schlagwörter: das Skalarprodukt mit allen Begriffsvektoren, Softmax mit dem Faktor 100. Übernommen werden höchstens `max_tags` Begriffe mit einer Wahrscheinlichkeit ≥ `min_confidence`, absteigend, jeweils mit `model = "clip-vit-b32"`.
  5. `description` bleibt leer.
- **Modell laden:** Das Plugin liest die Modelldatei bei jedem Aufruf über `fs-read` von `model_path`, prüft ihren SHA-256 gegen den eingebauten Wert und lädt sie mit `Model::load`. Eine fehlende Datei oder ein falscher SHA-256 ergibt `doc.ai = None` und eine Warnung mit Pfad, Ursache und dem Hinweis auf `make models`.
- **Settings:**
  - `model_path` (Standard `models/clip-vit-b32-vision-int8.onnx`, Pfad ohne `..`);
  - `max_tags` (Standard 5, Bereich 1–20);
  - `min_confidence` (Standard 0,10, Bereich 0,0–1,0);
  - `embedding_dim` ist nur noch 512, andere Werte ergeben eine Warnung;
  - `model`, `model_tier` und `device` werden weiterhin als nicht unterstützt gemeldet.
- **Konfiguration:**
  - `ai-tagging` ist in `config.yaml`, `config.mac-studio.yaml` und `AppConfig::default()` an;
  - dazu `resource_limits` mit `max_memory_bytes: 536870912` (512 MB) und `timeout_ms: 30000`;
  - der Hinweis „experimental“ entfällt.
- **Build und Release:**
  - `make test` hängt von `make models` ab;
  - `make dist` legt `models/clip-vit-b32-vision-int8.onnx` und `models/CLIP-LICENSE` in das Gesamtpaket und das Plattformpaket.
  - Host-Tests, die das Modell brauchen, brechen ohne Datei mit dem Hinweis „run `make models`“ ab.
- **Dokumentation:**
  - README des Plugins mit Lizenz und Einsatzhinweis der Modellkarte, ohne den Hinweis „experimental“;
  - `NOTICE` (CLIP, OpenAI, MIT; Konvertierung von Xenova);
  - Haupt-README, `docs/build-instructions.md` (`make models`, `make labels`), `docs/deployment-guide.md` (Modelldatei, Grenzen);
  - arc42 (Risiko 1 behoben), CHANGELOG, Beispielausgabe in `docs/pipeline_host_supervisor.md`.

## Nicht-Ziele

- Keine Bildbeschreibung (`description` bleibt leer).
- Keine Begriffe zu persönlichen Merkmalen, keine Gesichts- oder Personenerkennung.
- Keine Suche per Textanfrage gegen die Embeddings (Text-zu-Bild); dafür wäre der Textteil zur Laufzeit nötig.
- Kein anderes Modell, keine Modellwahl per Setting, kein Einbetten des Modells.
- Keine deutschen Schlagwörter.
- `config.docker.yaml` wird nicht angepasst.

## Akzeptanzkriterien

- **AC-1** Mit vorhandenem Modell liefert `ai-tagging` für „Apollo 11 Crew“ und „Expedition 64“ ein normiertes 512-dimensionales Embedding und Schlagwörter. Das jeweils beste Schlagwort stammt aus dem Themenbereich Raumfahrt der Begriffsliste (`a space suit`, `a spacecraft`, `outer space`, `a rocket`, `the earth from space`, `the moon`) und erreicht mindestens `min_confidence`. (Geändert am 2026-11-27: Zuerst stand hier „astronauts“ mit ≥ 0,5. „Astronaut“ ist ein Beruf und nach ADR 0010, Punkt 5 ausgeschlossen, und mit rund 280 Begriffen verteilt sich die Wahrscheinlichkeit breiter, gemessen 0,20–0,29 für das beste Schlagwort.)
- **AC-2** Die Kosinus-Ähnlichkeit der beiden NASA-Fotos ist größer als die jedes der beiden zu `strass.jpg` und zu `german-text.png`. `flagme-host similar` liefert nach der Verarbeitung aller vier Bilder zum Apollo-Foto als ersten Treffer das Expedition-Foto.
- **AC-3** Höchstens `max_tags` Schlagwörter, alle mit einer Wahrscheinlichkeit ≥ `min_confidence`, absteigend sortiert, mit `model = "clip-vit-b32"`. `description` ist leer.
- **AC-4** Fehlt die Modelldatei oder stimmt ihr SHA-256 nicht, bleibt `doc.ai = None`. Die Warnung nennt Pfad, Ursache und `make models`. Eine nicht lesbare Bilddatei, ein abgeschnittenes JPEG oder HEIC ergeben ebenfalls `None` mit Warnung.
- **AC-5** `make models` lädt die Dateien von der festen Revision, prüft ihre SHA-256 und bricht bei Abweichung ab, ohne die falsche Datei liegen zu lassen. `models/` steht in `.gitignore`.
- **AC-6** `labels.txt` enthält keinen Begriff aus einer im Test hinterlegten Sperrliste zu persönlichen Merkmalen (z. B. man, woman, girl, boy, child, old, young, black, white, asian, muslim, christian, beautiful, ugly, fat, disabled, nurse, doctor sowie Berufe wie astronaut, police, soldier). Jeder Begriff kommt nur einmal vor.
- **AC-7** Das Werkzeug `clip-labels` erzeugt aus denselben Eingaben byte-gleiche Ausgaben. `labels.bin` passt zu `labels.txt` (SHA-256 im Kopf, gleiche Anzahl). Das Plugin lehnt eine `labels.bin` mit falscher Version oder Dimension ab.
- **AC-8** Die Komponente im Host liefert nahezu dasselbe Ergebnis wie das Plugin nativ: Kosinus der Embeddings ≥ 0,99, dasselbe beste Schlagwort, und die Wahrscheinlichkeiten gemeinsamer Schlagwörter weichen höchstens um 0,05 ab. Das gilt innerhalb der Grenzen 512 MB und 30 s, auch für ein Bild von 4000 × 3000 Pixel. (Geändert am 2026-11-27: Zuerst stand hier Kosinus ≥ 0,999 und „dieselben Schlagwörter“. Die int8-Rechenkerne von `rten` runden nativ (ARM NEON) und in Wasm-SIMD verschieden; gemessen wurde ein Kosinus von 0,9968, und ein Schlagwort lag nativ mit 0,1016 knapp über, in Wasm knapp unter der Schwelle 0,10. Im Betrieb zählt nur die Wasm-Komponente.)
- **AC-9** `ai-tagging` ist in `config.yaml`, `config.mac-studio.yaml` und `AppConfig::default()` an, mit den Grenzen 512 MB und 30 s und ohne Hinweis „experimental“. `embedding_dim` ungleich 512 ergibt eine Warnung und den Wert 512.
- **AC-10** `make dist` legt Modelldatei und Lizenz in das Gesamtpaket und das Plattformpaket. Mit dem entpackten Plattformpaket liefert `process` für ein Foto ein Embedding.
- **AC-11** `NOTICE` und die README nennen CLIP, OpenAI, MIT und den Einsatzhinweis der Modellkarte.
