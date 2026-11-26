---
status: accepted
date: 2026-11-26
supersedes: []
references: []
---

# ADR 0002 — Plugins ohne echte Engine liefern keine Ergebnisse

## Kontext

Die Verfassung verbietet vereinfachte Mocks, Stubs und Pseudocode im Produktionspfad (_Strikte Regeln_ 2). Issue #23 zeigt, dass mehrere Plugins trotzdem Ergebnisse erfinden:

- `metadata` schreibt Demo-Werte (Sony ILCE-7RM4, GPS Berlin), wenn eine Datei kleiner als 128 Byte oder nicht lesbar ist.
- `ocr-text` leitet „erkannten Text“ aus Stichwörtern im Dateinamen oder aus dem Dateinamen selbst ab.
- `ai-tagging` erzeugt Tags aus Byte-Statistiken der komprimierten Datei, aus Dateinamen und aus festen Listen je „Modell“ (CLIP, MobileNet, OpenCLIP). Das Embedding stammt aus einem Byte-Histogramm und einem Pfad-Hash.
- `face-detection` liefert Gesichter aus einer Tabelle von Datei-Signaturen der Beispielfotos, aus Dateinamen-Regeln und aus einem schwachen Hautton-Detektor (über das externe Programm `djpeg`). Die Ergebnisse werden als „UltraFace“ bzw. „SCRFD-ArcFace“ ausgewiesen, Embeddings stammen aus Datei-Bytes.

Diese Daten landen in SQLite, im FTS5-Index und in der Ähnlichkeitssuche. `privacy-redact` verlässt sich für die DSGVO-Schwärzung auf `doc.faces`. Erfundene Gesichter erzeugen dort eine falsche Sicherheit.

Für OCR, Bild-Tagging und Gesichtserkennung gibt es im Projekt keine echte Inferenz. Sie braucht ein ML-Laufzeitsystem (z. B. `tract-onnx` in reinem Rust oder WASI-NN), Modelldateien mit Lizenz- und Verteilungsfragen und eine Abstimmung mit der Komponentenausführung (#30). Das ist eine eigene Architekturentscheidung.

## Entscheidung

1. **Kein Plugin erfindet Daten.** Lässt sich ein Wert nicht aus der Quelle bestimmen, bleibt das Feld leer (`None` bzw. leer) und das Plugin loggt eine Warnung. Das gilt für Lesefehler, nicht unterstützte Formate und fehlende Engines.
2. **Erkennung nur über austauschbare Engines.** `ocr-text`, `ai-tagging` und `face-detection` arbeiten ausschließlich über eine Engine-Schnittstelle (`OcrEngine`, `VisionModel`, `FaceDetector`), die nur die Bildbytes erhält. Das Ergebnis darf nicht vom Dateinamen oder Pfad abhängen. Eingebaute Pseudo-Engines gibt es nicht.
3. **Ohne Engine kein Ergebnis.** Solange keine echte Engine vorhanden ist, lassen diese Plugins `doc.ocr`, `doc.ai` bzw. `doc.faces` auf `None`. `None` bedeutet „nicht bestimmt“, `Some` mit leerer Liste „bestimmt, nichts gefunden“.
4. **Experimentell und standardmäßig aus.** Plugins ohne echte Engine gelten als experimentell und sind in allen mitgelieferten Konfigurationen (`config*.yaml`) mit `enabled: false` deaktiviert. Settings, die eine nicht vorhandene Engine auswählen (`model`, `model_tier`, `tier`, `engine`, `device`), werden mit der Warnung „not supported yet“ ignoriert.
5. **Datenschutz-Plugins machen fehlende Eingaben sichtbar.** Soll `privacy-redact` Gesichter schwärzen und fehlt ein Ergebnis der Gesichtserkennung (`doc.faces = None`), loggt es eine Warnung und meldet keine Schwärzung.
6. **Echte Engines folgen als eigenes Feature** mit eigenem ADR (Laufzeitsystem, Modellbezug, Lizenzen, Einbettung in die Wasm-Komponenten aus #30).

## Konsequenzen

- Positiv: Gespeicherte Tags, Texte, Gesichter und Metadaten sind entweder echt oder fehlen nachvollziehbar. Suche, Ähnlichkeitssuche und Schwärzung arbeiten nicht mehr mit erfundenen Daten.
- Positiv: Die Engine-Schnittstellen sind die Andockstelle für echte Modelle; Tests nutzen eigene Test-Engines außerhalb des Produktionspfads.
- Negativ: Bis echte Engines vorhanden sind, gibt es keine OCR, keine KI-Tags, keine Embeddings für `similar` und keine automatische Gesichtsschwärzung. Wer `privacy-redact` für Gesichter braucht, bekommt eine Warnung statt eines Ergebnisses.
- Negativ: Bestehende Datenbanken können erfundene Einträge aus früheren Läufen enthalten; sie werden erst durch erneute Verarbeitung ersetzt.
- Löst für diese drei Plugins die Modell-/Tier-Auswahl aus dem archivierten Feature `plugin-settings` ab (dort AC-6 für `device`/`engine` und AC-9). Kriterien bestehender PRDs, die eine eingebaute Erkennung voraussetzen (`face-detection` AC-3/AC-5, `ocr-text` AC-3/AC-5, `vector-search-embeddings` AC-2), werden auf die Engine-Schnittstelle präzisiert bzw. als entfallen markiert.
- Keine Verfassungsregel ändert sich; die Entscheidung präzisiert _Strikte Regeln_ 2.
