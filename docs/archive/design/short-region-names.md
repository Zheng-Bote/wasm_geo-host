---
feature: short-region-names
status: archived
archive-after:
touches:
  - tools/geonames-import/**
  - plugins/reversegeo/data/places.bin
  - plugins/reversegeo/README.md
  - crates/host/tests/**
  - docs/pipeline_host_supervisor.md
  - docs/arc42.md
  - CHANGELOG.md
release: 0.3.0
---

# Tech Spec — Kurze Regionsnamen im GeoNames-Ortsverzeichnis (`short-region-names`)

Setzt das PRD `docs/prd/short-region-names.md` um. Die Verfassung hat Vorrang; diese Tech Spec darf sie präzisieren, nie lockern. Grundlage ist [ADR 0008](../../adr/0008-short-region-names.md).

## Architektur

**`tools/geonames-import/src/lib.rs`**

- `Names::region(gid, lang, fallback) -> &str` (neu):
  1. `base = self.place(gid, lang).unwrap_or(fallback)`: der Grundname wie bisher.
  2. Kandidaten sind die Einträge derselben (`gid`, `lang`) mit `short = true`, deren Name `!= base` ist und für die `contains_words(base, name)` gilt.
  3. Ist die Liste leer, gilt `base`, sonst der Kandidat mit der kleinsten `id`.
- `fn contains_words(haystack, needle) -> bool` prüft, ob `needle` in `haystack` vorkommt (Groß- und Kleinschreibung zählen). An jeder Fundstelle muss davor der Anfang oder eines von ` `, `-`, `'`, `’`, `(` stehen, danach das Ende oder eines von ` `, `-`, `'`, `’`, `)`, `,`. Der Vergleich arbeitet auf `char`-Grenzen und ohne reguläre Ausdrücke.
- Die admin1-Schleife nutzt `names.region(a.geonameid, Some("en"), &a.name)` und `names.region(a.geonameid, lang, &a.name)`.
- admin2, Orte und Länder bleiben unverändert.

**`plugins/reversegeo`**

- Der Code bleibt unverändert, ebenso das Format (`FORMAT_VERSION = 1`).
- Nur `data/places.bin` wird neu erzeugt:

  ```bash
  cargo run --release -p geonames-import -- --input <Dumps vom 2026-11-27> --output plugins/reversegeo/data/places.bin --date 2026-11-27
  ```

  Die Dumps liegen beim Maintainer; `make geonames` würde neue Dumps laden und ist hier nicht gewollt (PRD, Nicht-Ziele).

## Schnittstellen

Unverändert. Nur die Werte von `region` und `local_region` ändern sich.

## Datenmodell

`places.bin` behält Format, Kopf und Datum. Die Tabelle der Zeichenketten kann kleiner werden, weil kurze Namen oft schon als Ortsnamen vorhanden sind („Salzburg“) und deshalb geteilt werden.

## Umsetzung der Akzeptanzkriterien

| AC   | Umsetzung                       | Test                                                                                                                      |
| ---- | ------------------------------- | ------------------------------------------------------------------------------------------------------------------------- |
| AC-1 | `Names::region`                 | `crates/host/tests/short_region_names_test.rs` `test_ac1_short_region_names`                                              |
| AC-2 | Rückfall auf den Grundnamen     | `test_ac2_base_name_without_matching_short_form`                                                                          |
| AC-3 | admin2 unverändert              | `test_ac3_counties_unchanged`                                                                                             |
| AC-4 | `contains_words`, kleinste `id` | `tools/geonames-import/tests/import_test.rs` `test_ac4_short_form_rule` (Fixture)                                         |
| AC-5 | neu erzeugte `places.bin`       | `short_region_names_test.rs` `test_ac5_regenerated_data`; Beispielausgabe über `examples_test.rs` (`wasm-geo-host` AC-12) |
| AC-6 | README, CHANGELOG               | `test_ac6_documentation`                                                                                                  |

Der bestehende Test `geonames-gazetteer` AC-1 erwartet `local_region` `Bayern`. Das bleibt so, weil Bayern keine kürzere Form hat.

## Sicherheit

Keine Auswirkung. Das Werkzeug bleibt ohne `unsafe`, das Plugin unverändert. Jeder Name stammt unverändert aus GeoNames (_Strikte Regeln_ 2).

## Messwerte

Neu erzeugt am 2026-11-27 aus denselben Dumps (Datum 2026-11-27).

| | vorher | nachher |
| :-- | --: | --: |
| `places.bin` | 7.005.345 Byte | 6.986.514 Byte |
| Zeichenketten | 205.294 | 204.514 |
| admin1 mit geändertem englischem Namen | — | 871 von 3.865 |
| admin1 mit geändertem lokalem Namen | — | 333 von 3.865 |

## Offene Punkte

- Keine.
