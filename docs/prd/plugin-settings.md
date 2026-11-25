---
feature: plugin-settings
status: accepted
archive-after:
touches:
  - plugins/**
  - crates/sdk/**
  - crates/host/src/config.rs
  - crates/host/tests/**
  - config*.yaml
---

# PRD — Plugin-Settings: alle Plugins werten config.yaml-Settings aus (`plugin-settings`)

## Problem

Der Host übergibt jedem Plugin die `settings` seines Eintrags in `config.yaml` als `PluginContext.settings` (`HashMap<String, String>`). Ausgewertet werden sie heute nur von `privacy-redact`. Alle anderen Plugins arbeiten mit fest im Code hinterlegten Werten.

Die mitgelieferten Profile `config.docker.yaml` und `config.mac-studio.yaml` setzen dennoch Schlüssel für sieben weitere Plugins (z. B. `tier`, `min_confidence`, `model`, `quality`, `phash_threshold`). Diese Schlüssel bleiben **wirkungslos und ohne Hinweis**. Betreiber gehen davon aus, ein Profil (z. B. „Heavy“ auf dem Mac Studio) aktiviert zu haben, obwohl das Verhalten identisch zum Standard ist.

Zusätzlich gilt in `privacy-redact` eine überraschende Regel: nur `true` ist wahr, `"yes"` oder `"1"` schalten eine Funktion **ab**; ungültige Werte werden stillschweigend verworfen.

Bezug: Issue #26.

## Ziel

- Jedes Plugin definiert seine Parameter als typisierte Settings-Struktur mit Standardwerten und wertet `ctx.settings` bei jedem Aufruf aus.
- Jeder in den mitgelieferten Profilen gesetzte Schlüssel ändert das Verhalten des Plugins nachweisbar — oder wird entfernt.
- Ungültige Werte und unbekannte Schlüssel werden mit einer Warnung gemeldet und fallen auf den Standardwert zurück.
- Alle Plugins verwenden dieselben Regeln zum Einlesen von Werten (gemeinsame Hilfsfunktionen im SDK).
- Jede Plugin-README dokumentiert alle Schlüssel mit Typ, Standard, Wertebereich und Wirkung.

## Nutzer

- Betreiber von `flagme-host`, die über `config.yaml` und die Profile (Docker, Mac Studio) das Verhalten der Pipeline steuern.
- Plugin-Entwickler, die neue Parameter nach einem einheitlichen Muster ergänzen.

## Umfang

### Gemeinsame Regeln (SDK, `crates/sdk`)

- Hilfsfunktionen zum Einlesen von `bool`, Ganzzahlen, Fließkommazahlen (jeweils mit Wertebereich), Aufzählungen (mit Aliasen), Listen (kommagetrennt) und Pfaden (ohne `..`).
- **Bool-Regel für alle Plugins:** `true`/`yes`/`on`/`1` → wahr, `false`/`no`/`off`/`0` → falsch (Groß-/Kleinschreibung egal). Jeder andere Wert → Warnung und Standardwert.
- Zahlen außerhalb des Wertebereichs werden auf die Grenze gesetzt **und** mit Warnung gemeldet.
- Kanonischer Schlüssel hat Vorrang vor einem Alias.
- Unbekannte Schlüssel → eine Warnung je Schlüssel über `ctx.capabilities.log("warn", …)`.

### Parameter je Plugin

| Plugin           | Schlüssel (Standard)                                                                                                                                                                                  |
| :--------------- | :---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `metadata`       | `makernote_max_length` (512), `xmp_max_value_length` (512)                                                                                                                                            |
| `reversegeo`     | `offline_cache` (true), `offline_lookup` (true), `online_lookup` (true), `city_radius_km` (200), `cache_precision` (3), `nominatim_url` (Nominatim-Reverse-Endpunkt), `accept_language` (`en`)        |
| `countryflag`    | keine Parameter; unbekannte Schlüssel werden gemeldet                                                                                                                                                 |
| `thumbnail`      | `variants` (`128,512,1024`), `max_dimension` (keine Grenze), `quality` (1–100, ohne Wert Encoder-Standard), `output_dir` (`cache/thumbnails`)                                                         |
| `ai-tagging`     | `model` (`CLIP-ViT-B/32` \| `MobileNetV3` \| `OpenCLIP-ConvNeXt-Large`), `model_tier` (`lightweight` → MobileNetV3, `heavy` → OpenCLIP; `model` hat Vorrang), `embedding_dim` (512), `device` (`cpu`) |
| `duplicate`      | `phash_threshold` (0, Hamming-Distanz 0–64)                                                                                                                                                           |
| `timeline`       | `timezone` (`UTC`; IANA-Name oder Offset für EXIF-Zeiten ohne Offset), `source_priority` (Reihenfolge der Quellen aus der README)                                                                     |
| `weather-enrich` | `endpoint` (Open-Meteo-Archiv), `coordinate_precision` (2), `offline_fallback` (true)                                                                                                                 |
| `ocr-text`       | `engine` (`default`), `min_confidence` (0.0)                                                                                                                                                          |
| `face-detection` | `tier` (`lightweight` \| `heavy`), `min_confidence` (0.0), `device` (`cpu`)                                                                                                                           |
| `privacy-redact` | bestehende Schlüssel; Umstellung auf die gemeinsamen Regeln                                                                                                                                           |

`ocr-text.max_image_dimension` entfällt: ohne Bilddekodierung ist der Wert nicht ehrlich umsetzbar; er folgt mit Issue #23.

Werte, die eine noch nicht vorhandene Implementierung verlangen (z. B. `device: mps`, `engine: PaddleOCR-v4-Server`), werden **nicht** vorgetäuscht: Das Plugin meldet eine Warnung, nennt die unterstützten Werte und nutzt den Standard (Verfassung, Strikte Regeln 2).

### Host und Profile

- Der YAML-Leser in `crates/host/src/config.rs` behandelt `#` innerhalb von Anführungszeichen nicht mehr als Kommentarbeginn (z. B. für URLs).
- `config.yaml`, `config.docker.yaml` und `config.mac-studio.yaml` enthalten nur noch Schlüssel, die ein Plugin auswertet.
- Plugin-READMEs: Abschnitt „§2.2 `settings`“ wird je Plugin zur vollständigen Referenztabelle.

## Nicht-Ziele

- Keine echten neuronalen Modelle, keine GPU-/MPS-Anbindung (Issue #23).
- Keine Behebung der Capability-Umgehungen (Issue #25) und der Gesichts-Schwärzung (Issue #22) — nur deren Parameter.
- Kein Neuladen der Konfiguration zur Laufzeit.
- Keine Änderung des WIT-Vertrags; `settings` bleibt eine Zeichenketten-Map.

## Akzeptanzkriterien

- **AC-1** Das SDK stellt Hilfsfunktionen für `bool`, Ganzzahl, Fließkomma, Aufzählung, Liste und Pfad bereit; jede Funktion liefert bei ungültigem Wert den Standard und eine Warnung.
- **AC-2** Die Bool-Regel gilt in allen Plugins einheitlich: `true/yes/on/1` und `false/no/off/0` werden erkannt, andere Werte erzeugen eine Warnung und den Standardwert.
- **AC-3** Ein unbekannter Schlüssel unter `settings` erzeugt genau eine Warnung mit Plugin- und Schlüsselnamen; die Verarbeitung läuft weiter.
- **AC-4** Jeder Schlüssel aus der Tabelle „Parameter je Plugin“ ändert das Ergebnis des Plugins nachweisbar (mindestens ein Test je Schlüssel).
- **AC-5** Ohne `settings` liefern alle Plugins exakt dasselbe Ergebnis wie vor der Änderung (Rückwärtskompatibilität), mit Ausnahme der in AC-2 erweiterten Bool-Regel.
- **AC-6** Ein Wert, der eine nicht vorhandene Implementierung verlangt (z. B. `device: mps`), erzeugt eine Warnung mit den unterstützten Werten und nutzt den Standard.
- **AC-7** `duplicate` erkennt mit `phash_threshold: N` zwei Bilder als Duplikat, deren dHash sich in höchstens `N` Bits unterscheidet.
- **AC-8** `face-detection` und `ocr-text` liefern mit `min_confidence: X` keine Einträge mit `confidence < X`.
- **AC-9** `ai-tagging` (`model`, `model_tier`) und `face-detection` (`tier`) wählen die Implementierung pro Aufruf aus `ctx.settings`; der Host muss dafür nicht geändert werden.
- **AC-10** Der Host-YAML-Leser übernimmt `#` innerhalb von Anführungszeichen unverändert in den Wert.
- **AC-11** Die drei mitgelieferten Profile enthalten nur ausgewertete Schlüssel; das Laden jedes Profils erzeugt keine Warnung wegen unbekannter Schlüssel.
- **AC-12** Jede Plugin-README listet alle Schlüssel mit Typ, Standard, Wertebereich und Wirkung; `sdd gate pre-merge` läuft ohne Fehler.
