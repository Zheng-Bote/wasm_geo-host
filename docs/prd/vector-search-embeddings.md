---
feature: vector-search-embeddings
status: draft
archive-after:
touches:
  - "crates/models/**"
  - "plugins/ai-tagging/**"
  - "crates/host/**"
  - "crates/host/migrations/**"
---

# PRD — vector-search-embeddings (`vector-search-embeddings`)

## Problem

Herkömmliche schlagwortbasierte Suchen und exakte Tag-Abgleiche finden nur Fotos, deren Labels exakt mit den Suchbegriffen übereinstimmen. Häufig suchen Nutzer jedoch mit natürlicher Sprache oder abstrakteren visuellen Beschreibungen ("rotes Oldtimer-Auto vor einer Scheune", "nebliger Herbstmorgen im Wald"), für die keine vordefinierten Tags existieren.

## Ziel

Ermöglichung einer semantischen Vektorähnlichkeitssuche (Vector Search) durch Berechnung und Speicherung von dichten 512-dimensionalen Vektoreinbettungen (CLIP Image Embeddings). Die Speicherung erfolgt platzsparend als binärer SQLite-BLOB in `photo_embeddings`, die Ähnlichkeitsberechnung erfolgt über Kosinus-Ähnlichkeit (Cosine Similarity) im Host, und ein CLI-Subcommand `flagme-host similar <PHOTO_ID>` liefert die Top-K ähnlichsten Medien.

## Nutzer

- Endnutzer und Entwickler, die visuell und thematisch ähnliche Bilder auffinden wollen, ohne auf feste Tags angewiesen zu sein.
- Medien- und Archivierungs-Pipelines zur Duplikats- und Varianten-Erkennung sowie zur semantischen Kategorisierung.

## Umfang

Die Umsetzung umfasst:
- Erweiterung von `AiInfo` in `wasm-geo-models` um ein optionales `embedding: Option<Vec<f32>>`.
- Erweiterung des VisionModel-Traits und `ai-tagging`-Plugins um Vektorexport (z. B. normierte 512-dimensionale Float-Vektoren).
- Schema-Erweiterung in SQLite um Tabelle `photo_embeddings` (BLOB-Speicherung).
- Erweiterung von `Storage` um Persistenz und Abfrage von Binär-BLOB-Embeddings sowie Kosinus-Ähnlichkeits-Matching.
- CLI-Subcommand `flagme-host similar <PHOTO_ID> [--limit <N>] [--config <CONFIG>]`.
- Integrationstests je Kriterium mit Kennzeichnung.

## Nicht-Ziele

- Keine externe Vektordatenbank (Pinecone, Milvus, Qdrant); der Betrieb erfolgt vollkommen lokal und eingebettet in SQLite.
- Kein Text-zu-Bild-Inferenzmodell zur Laufzeit im CLI (Suche basiert auf Bild-zu-Bild-Ähnlichkeit via Vorberechnung).

## Akzeptanzkriterien

- **AC-1** Model Extension: `AiInfo` in `wasm-geo-models` unterstützt optionale dichte Vektoreinbettungen (`embedding: Option<Vec<f32>>`).
- **AC-2** Plugin Embedding Export: Das `ai-tagging`-Plugin exportiert die Vektoren seines `VisionModel` normiert und 512-dimensional im `UnifiedDocument` ([ADR 0002](../adr/0002-plugins-without-engine-no-output.md)).
- **AC-3** SQLite Binary BLOB Storage: `Storage` persistiert und lädt Vektor-Embeddings als Binär-BLOBs (Little-Endian IEEE 754 Floats) in `photo_embeddings`.
- **AC-4** Similar CLI Subcommand: Der CLI-Befehl `flagme-host similar <PHOTO_ID>` berechnet die Kosinus-Ähnlichkeit über alle gespeicherten Einbettungen und liefert die Top-K ähnlichsten Fotos.
