---
status: accepted
date: 2026-11-27
supersedes: []
references:
  - docs/adr/0002-plugins-without-engine-no-output.md
  - docs/adr/0003-wasm-component-execution.md
  - docs/adr/0009-plugin-inference-rten.md
---

# ADR 0010 — ai-tagging mit CLIP ViT-B/32 als Modelldatei

## Kontext

`ai-tagging` ist das letzte Plugin ohne Engine (ADR 0002). Ohne es gibt es keine Schlagwörter und keine Embeddings, also auch keine Ähnlichkeitssuche mit `flagme-host similar`.

[ADR 0009](0009-plugin-inference-rten.md) legt die Regeln für echte Engines fest: `rten` im Gast, Modelle mit freier Lizenz, die kommerzielle Nutzung und Weitergabe erlaubt, und Einbetten nur bis 16 MB. Abweichungen von der Größengrenze (Punkt 3) regelt eine eigene ADR. Das ist diese.

**Prototyp am 2026-11-27** mit OpenAI CLIP ViT-B/32. Die ONNX-Fassung stammt von `Xenova/clip-vit-base-patch32`, Revision `6b7c523703ae26e39be4b03774966f8571c7f523`. Gemessen wurde auf macOS (Apple Silicon) im Release-Build mit `rten` 0.28; Bildteil (`vision_model_int8.onnx`, 88.648.877 Byte), Ausgabe ein 512-dimensionales Embedding.

|                                                             |    nativ | `wasm32-wasip2` mit SIMD |
| :---------------------------------------------------------- | -------: | -----------------------: |
| Modell laden (Datei lesen und parsen)                       |    41 ms |                    58 ms |
| Embedding je Foto (Dekodieren, 224 × 224 zuschneiden, Lauf) | 28–73 ms |               367–425 ms |
| höchster Speicher des ganzen `wasmtime`-Prozesses           |        — |                   372 MB |

**Schlagwörter** (Zero-Shot gegen 24 Testbegriffe, Prompt „a photo of …“, Softmax mit Faktor 100):

| Bild                                      | beste Begriffe                       |
| :---------------------------------------- | :----------------------------------- |
| Apollo 11 Crew                            | astronauts 0,91, a space suit 0,08   |
| Expedition 64                             | astronauts 0,95, a space suit 0,04   |
| Ortsschild Straß                          | a street sign 0,61, a town sign 0,37 |
| Buchcover                                 | a book cover 0,84                    |
| Stromzähler (Foto des Maintainers, lokal) | an electricity meter 1,00            |
| gerenderter Text                          | keine klare Zuordnung (≤ 0,11)       |

**Ähnlichkeit** (Kosinus): Die beiden Astronautenfotos erreichen 0,78, alle anderen Paare 0,21–0,52.

**Lizenz und Einsatzhinweis:**

- Code und Gewichte von CLIP stehen unter MIT (Copyright (c) 2021 OpenAI, `github.com/openai/CLIP`); die ONNX-Konvertierung von Xenova ebenfalls.
- Die Modellkarte von OpenAI rät jedoch von jedem produktiven Einsatz ab („Any deployed use case of the model … is currently out of scope“). Eine Bildsuche in begrenzter Umgebung hält sie nur nach gründlichen Tests mit einer **festen Begriffsliste** für vertretbar. Sie warnt vor Verzerrungen, besonders bei Begriffen über Personen.
- Das ist eine Empfehlung, keine Lizenzbedingung.

**Alternativen:**

- MobileCLIP S0 (12 MB) steht unter `apple-amlr`, einer Lizenz für Apples Forschung. Es erfüllt die Lizenzregel von ADR 0009 nicht.
- SigLIP-Fassungen haben auf Hugging Face keine Lizenzangabe und sind über 200 MB groß.
- Die fp32-Fassung von CLIP ist 352 MB groß.

## Varianten

### Auslieferung des Modells (> 16 MB)

| Variante                                                              | Vorteile                                                                                                                                                                                                                          | Nachteile                                                                                                                                                                                                                             |
| :-------------------------------------------------------------------- | :-------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | :------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| **A. Modelldatei, geladen mit `make models`, gelesen über `fs-read`** | <ul><li>Keine 89 MB in Git.</li><li>Die Komponente bleibt klein.</li><li>`ai-tagging` hat den Grant `filesystem` schon.</li><li>Die Release-Pakete können die Datei mitliefern.</li><li>SHA-256 und Revision sind fest.</li></ul> | <ul><li>Je Foto werden 89 MB über die WIT-Grenze gelesen (im Prototyp enthalten in den 58 ms).</li><li>Datei und Code können auseinanderlaufen; das fängt die SHA-256-Prüfung ab.</li><li>Ohne `make models` kein Ergebnis.</li></ul> |
| **B. Einbetten mit Git LFS**                                          | <ul><li>Datei und Code passen immer zusammen.</li></ul>                                                                                                                                                                           | <ul><li>Die Komponente wäre ≈ 93 MB groß; unter macOS je Instanz 89 MB kopieren.</li><li>LFS-Einrichtung, begrenztes Kontingent.</li></ul>                                                                                            |
| **C. Einbetten direkt in Git**                                        | <ul><li>Einfach.</li></ul>                                                                                                                                                                                                        | <ul><li>Jede Modelländerung bläht die Historie um 89 MB auf.</li></ul>                                                                                                                                                                |

### Schlagwörter

| Variante                                                               | Vorteile                                                                                                                                                                                          | Nachteile                                                                                  |
| :--------------------------------------------------------------------- | :------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | :----------------------------------------------------------------------------------------- |
| **A. Eigene, feste Begriffsliste** mit vorab berechneten Text-Vektoren | <ul><li>Passt zu Fotosammlungen.</li><li>Entspricht der Empfehlung der Modellkarte (feste, getestete Taxonomie).</li><li>Nur ≈ 0,5 MB Vektoren im Plugin; kein Textmodell zur Laufzeit.</li></ul> | <ul><li>Liste und Vektoren müssen gepflegt werden; ein Werkzeug rechnet sie neu.</li></ul> |
| **B. Standardkategorien** (ImageNet, Places365)                        | <ul><li>Etabliert.</li></ul>                                                                                                                                                                      | <ul><li>Passt wenig zu privaten Fotos (z. B. 120 Hunderassen).</li></ul>                   |
| **C. Nur Embeddings**                                                  | <ul><li>Am einfachsten.</li></ul>                                                                                                                                                                 | <ul><li>Keine Suche nach Begriffen.</li></ul>                                              |

## Entscheidung

1. **Modell.** `ai-tagging` nutzt den Bildteil von OpenAI CLIP ViT-B/32 in der int8-Fassung (`vision_model_int8.onnx`, `Xenova/clip-vit-base-patch32`, Revision `6b7c523703ae26e39be4b03774966f8571c7f523`, SHA-256 `0ab0c1b3ace708e539633af1744d5a95247fe4e14d3e08ff197ef82a6cb9bd93`, MIT). Es läuft nach ADR 0009 mit `rten` im Gast.
2. **Ausnahme von ADR 0009, Punkt 3: Modelldatei statt Einbetten (A).**
   - `make models` lädt die Datei von der festen Revision in `models/`, prüft den SHA-256 und bricht bei Abweichung ab.
   - `models/` steht in `.gitignore`. Die Release-Pakete enthalten die Datei samt Lizenz.
   - `ai-tagging` liest sie über `fs-read` vom Setting `model_path` (Standard `models/clip-vit-b32-vision-int8.onnx`, relativ zum Arbeitsverzeichnis) und prüft vor dem Laden den SHA-256.
   - Fehlt die Datei oder stimmt der SHA-256 nicht, warnt das Plugin und lässt `doc.ai = None` (ADR 0002).
   - Die Grenze von 16 MB aus ADR 0009 bleibt für alle anderen Modelle bestehen.
3. **Embedding.**
   - `ai.embedding` ist der normierte 512-dimensionale Bildvektor von CLIP; `flagme-host similar` nutzt ihn.
   - `embedding_dim` kann nur 512 sein; andere Werte werden mit einer Warnung ignoriert.
4. **Schlagwörter aus einer festen Begriffsliste (A).**
   - Eine gepflegte Liste englischer Begriffe für Fotosammlungen liegt im Repository.
   - Ein Werkzeug im Workspace rechnet sie nativ mit dem Textteil von CLIP (`text_model_int8.onnx`, gleiche Revision, über `make models`) in normierte Vektoren um. Es nutzt als Prompt „a photo of …“.
   - Die Vektoren werden im Plugin eingebettet (≈ 0,5 MB); zur Laufzeit gibt es kein Textmodell.
   - `ai.tags` sind die Begriffe mit der höchsten Wahrscheinlichkeit (Softmax mit dem CLIP-Faktor 100) über einer Schwelle, mit `model = "clip-vit-b32"`.
5. **Keine Begriffe über persönliche Merkmale.** Die Liste enthält keine Begriffe zu Geschlecht, Alter, Herkunft, Hautfarbe, Religion, Aussehen, Gesundheit oder Beruf von Personen. Erlaubt sind neutrale Begriffe wie „people“, „a group of people“ oder „a portrait“. Das setzt die Warnung der Modellkarte zu Verzerrungen bei Personen um.
6. **Keine Bildbeschreibung.** CLIP erzeugt keinen Text. `ai.description` bleibt leer; ein Text aus den Schlagwörtern wird nicht gebildet (ADR 0002).
7. **Standard und Grenzen.**
   - `ai-tagging` ist in den mitgelieferten Konfigurationen an.
   - Es hat eigene `resource_limits` (Speicher 1 GB, Zeit 30 s). Die genauen Werte legt das PRD nach Messung fest.
   - Der Hinweis „experimental“ entfällt (ADR 0002, Entscheidung 4).
8. **Hinweis zum Einsatz.** README und `NOTICE` nennen die Lizenz (MIT) und den Einsatzhinweis der Modellkarte. Die Ergebnisse dienen der privaten Sortierung und Suche, nicht automatischen Entscheidungen über Personen.

## Konsequenzen

**Positiv**

- Schlagwörter und Ähnlichkeitssuche funktionieren erstmals echt, offline und in der Sandbox. Damit hat jedes Plugin eine echte Engine (arc42, Risiko 1).
- Keine 89 MB in Git, die Komponente bleibt klein, und die Modellversion ist über Revision und SHA-256 fest.
- Die feste Begriffsliste ohne persönliche Merkmale begrenzt die Risiken, vor denen die Modellkarte warnt.

**Negativ**

- Ohne `make models` (bzw. das Release-Paket) liefert `ai-tagging` nichts. Erstmals hängt ein Plugin von einer Datei außerhalb der Komponente ab.
- Etwa 0,4 s und mehrere hundert MB Speicher je Foto; dafür braucht das Plugin eigene, höhere Grenzen.
- Die Release-Pakete werden um ≈ 89 MB größer.
- Die Modellkarte rät vom produktiven Einsatz ab. Das Projekt folgt ihr nur teilweise, mit fester Liste und lokalem Einsatz; die Verantwortung für den Einsatz liegt beim Betreiber.
- Die Begriffsliste und ihre Vektoren müssen bei Änderungen neu berechnet werden.

**Verfassung**

- Keine Regel ändert sich. _Strikte Regeln_ 1 (Rust, `rten`, kein `unsafe`) und _Security_ 1–3 bleiben gewahrt: Der Grant `filesystem` bestand schon, und es gibt eigene Grenzen.
- ADR 0009 bleibt gültig; diese ADR ist die dort in Punkt 3 vorgesehene Ausnahme für ein großes Modell.
