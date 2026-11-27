---
feature: short-region-names
status: accepted
archive-after:
touches:
  - tools/geonames-import/**
  - plugins/reversegeo/data/places.bin
  - plugins/reversegeo/README.md
  - crates/host/tests/**
  - docs/pipeline_host_supervisor.md
  - docs/arc42.md
  - CHANGELOG.md
---

# PRD — Kurze Regionsnamen im GeoNames-Ortsverzeichnis (`short-region-names`)

## Problem

`reversegeo` liefert Regionsnamen in der langen amtlichen Form, wie GeoNames sie bevorzugt (Issue #67). Beispiele: „State of Salzburg“, „Canton of Geneva“, „Free and Hanseatic City of Hamburg“, „Land Berlin“, „Kanton Basel-Stadt“. In Datenbank, Suche, Web-Oberfläche und Ordnernamen sind sie schwer lesbar.

## Ziel

`region` und `local_region` nehmen die GeoNames-Kurzform, wenn sie im bisherigen Namen wörtlich enthalten ist ([ADR 0008](../adr/0008-short-region-names.md)). Sonst bleibt der bisherige Name. Es wird kein Name gebildet oder gekürzt.

## Nutzer

- **Fotografen und Betreiber:** Sie sehen kurze, gebräuchliche Regionsnamen in Suche, Karte, Galerie und Zielordnern.

## Umfang

- **`tools/geonames-import`:** neue Namenswahl für admin1, englisch und in der Landessprache (ADR 0008, Entscheidung 4). Zuerst wird der Grundname bestimmt, wie bisher. Gibt es eine Kurzform (`isShortName = 1`) in derselben Sprache, die im Grundnamen als ganze Wortfolge vorkommt, gilt sie; bei mehreren die mit der kleinsten `alternateNameId`.
- **`places.bin`:** aus denselben Dumps vom 2026-11-27 neu erzeugt, mit dem gleichen Datum. Formatversion und Format bleiben gleich, `reversegeo` ändert sich nicht.
- **Dokumentation:** README von `reversegeo` (Namensregel, Beispiele, Berlin), Beispielausgabe in `docs/pipeline_host_supervisor.md`, arc42 (ADR-Tabelle) und CHANGELOG.

## Nicht-Ziele

- Keine Änderung an `county`, `city`, `local_city`, `country` und `local_country`.
- Keine handgepflegten Namen; „State of Berlin“ bleibt.
- Keine neuen Dumps und kein neues Datum der Daten.
- Bereits gespeicherte Dokumente werden nicht nachträglich umbenannt.

## Akzeptanzkriterien

- **AC-1** Offline bekommen Punkte in Salzburg, Wien, Genf, Basel, Hamburg, Paris und Prag diese Regionsnamen (englisch / lokal): `Salzburg`/`Salzburg`, `Vienna`/`Wien`, `Geneva`/`Genf`, `Basel-City`/`Basel-Stadt`, `Hamburg`/`Hamburg`, `Île-de-France`/`Île-de-France`, `Prague`/`Praha`.
- **AC-2** Ohne passende Kurzform bleibt der bisherige Name. Berlin heißt `State of Berlin` / `Berlin`, Mainz `Rhineland-Palatinate` / `Rheinland-Pfalz`, und Bayern bleibt `Bavaria` / `Bayern`.
- **AC-3** Kreise bleiben unverändert: Brighton hat den Kreis `Brighton and Hove`, München `Upper Bavaria`.
- **AC-4** Das Werkzeug übernimmt eine Kurzform nur als ganze Wortfolge des Grundnamens: aus `State of Salzburg` wird `Salzburg`, eine Kurzform `Salz` greift nicht, eine nicht enthaltene Kurzform greift nicht. Bei mehreren gilt die kleinste `alternateNameId`, und die Ausgabe bleibt byte-gleich bei gleichen Eingaben.
- **AC-5** `places.bin` ist mit Formatversion 1 und dem Datum 2026-11-27 neu erzeugt und höchstens 8 MiB groß. Die dokumentierte Beispielausgabe stimmt mit einem echten Lauf überein.
- **AC-6** Die README von `reversegeo` beschreibt die Regel mit Beispielen und nennt Berlin als Fall ohne englische Kurzform. Das CHANGELOG nennt die Änderung.
