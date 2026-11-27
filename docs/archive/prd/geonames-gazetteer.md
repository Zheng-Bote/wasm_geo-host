---
feature: geonames-gazetteer
status: archived
archive-after:
touches:
  - plugins/reversegeo/**
  - tools/geonames-import/**
  - Cargo.toml
  - Cargo.lock
  - Makefile
  - NOTICE
  - README.md
  - CHANGELOG.md
  - crates/host/tests/**
  - docs/arc42.md
  - docs/build-instructions.md
release: 0.3.0
---

# PRD — Offline-Ortsverzeichnis aus GeoNames für reversegeo (`geonames-gazetteer`)

## Problem

`reversegeo` kennt offline nur 35 Referenzstädte (9 in Deutschland) und 7 Länder-Rechtecke (Issue #64). Daraus entstehen drei Fehler:

- **Kleinstädte fehlen:** Fotos aus Freising oder Moosburg heißen „Munich“.
- **Falsches Land (#63):** Fotos jenseits einer Grenze bekommen das Land der nächsten Referenzstadt. Prag wird zu „Dresden, Germany“, Garmisch-Partenkirchen zu „Innsbruck, Austria“.
- **Erfundene Hauptstadt (#62):** Fotos fern jeder Referenzstadt bekommen die Hauptstadt ihres Länder-Rechtecks. Chicago wird zu „San Francisco“, Palermo zu „Rome“.

Die falschen Namen landen in der Datenbank, im KV-Cache und in den Zielordnern des Watchers (`<Country>/<City>/<Year>`). Genaue Namen gibt es heute nur online über Nominatim. Dafür verlassen die Koordinaten jedes Fotos das Gerät.

## Ziel

`reversegeo` ordnet Fotos offline dem nächsten besiedelten Ort aus GeoNames `cities1000` zu, mit seiner Region und seinem Land ([ADR 0006](../../adr/0006-geonames-gazetteer-names.md), ersetzt [ADR 0005](../../adr/0005-geonames-gazetteer.md)).

- Die Namen kommen in Englisch und in der Landessprache.
- Liegt kein Ort nahe genug, fragt das Plugin online oder lässt die Namen leer und warnt.
- Erfundene oder aus einer fernen Stadt übernommene Orte gibt es nicht mehr. Das behebt #62 und #63.

## Nutzer

- **Fotografen und Betreiber:** Sie bekommen in der Datenbank und in der Ordnerstruktur echte Orte, auch Kleinstädte, ohne Online-Dienst.
- **Betreiber mit Datenschutzbedarf:** Sie können `online_lookup` ausschalten und behalten trotzdem genaue Namen.
- **Maintainer:** Sie aktualisieren die Daten mit `make geonames`.

## Umfang

- **Aufbereitung** (`tools/geonames-import`, Rust, Workspace-Mitglied, nicht Teil einer Komponente):
  - Liest `cities1000.txt`, `admin1CodesASCII.txt`, `admin2Codes.txt`, `countryInfo.txt` und `alternateNamesV2.txt`.
  - Verwirft Ortsteile (`PPLX`).
  - Schreibt `plugins/reversegeo/data/places.bin`. Der Kopf enthält Formatversion, Quelldateien und das Datum der Dumps.
  - Gleiche Eingaben ergeben byte-gleiche Ausgaben.
- **`make geonames`:** Lädt die Dumps in einen Ordner außerhalb des Repositorys und ruft das Werkzeug auf. Danach liegt `places.bin` als Änderung zum Committen bereit.
- **`places.bin`:** Die Datei wird committet. Der normale Build und `make release` brauchen kein Netz.
- **`reversegeo`:**
  - Bettet `places.bin` mit `include_bytes!` ein und liest es ohne Aufbau und ohne `unsafe`.
  - Sucht den nächsten Ort nach Großkreisentfernung innerhalb von `city_radius_km`.
  - Übernimmt dessen `city`, `county`, `region`, `country`, `country_code`, `local_city`, `local_region`, `local_country` und `local_language`.
- **Namensregeln** (ADR 0006, Punkt 4):
  - Die englischen Felder nehmen den bevorzugten englischen Namen, sonst den GeoNames-Hauptnamen; `country` kommt aus `countryInfo.txt`.
  - `local_city` und `local_region` nehmen den bevorzugten Namen in der ersten Landessprache, sonst den GeoNames-Hauptnamen desselben Orts bzw. derselben Verwaltungseinheit.
  - `local_country` nimmt den Namen des Landes in der Landessprache, Kurzform zuerst; fehlt er, bleibt das Feld leer.
  - In mehrsprachigen Ländern gilt die erste Landessprache für alle Orte (Genf → `Genf`, `de`); die README nennt das als Einschränkung.
- **`city_radius_km` bekommt eine neue Bedeutung:** Es ist die Höchstentfernung zum nächsten Ort.
  - Der Bereich ist 1–200, der Standard ist **25** (bisher 200 um 35 Referenzstädte).
  - Gemessen mit `cities1000`: Für 95,6 % eines 0,25°-Rasters über Deutschland und die Alpen liegt ein Ort innerhalb von 25 km; der Rest ist vor allem Nord- und Ostsee.
  - Ein Wert über 200 wird nach den SDK-Regeln mit einer Warnung auf 200 gekappt.
- **Alte Tabelle:** `CITIES` und `COUNTRY_BOXES` entfallen ersatzlos.
- **Ohne Treffer:** Liegt kein Ort innerhalb von `city_radius_km`, gilt der bestehende Weg:
  - mit `online_lookup` die Anfrage an Nominatim;
  - sonst bleiben die Namen leer, das Plugin warnt und schreibt nichts in den Cache.
- **Andere Settings unverändert:** `offline_cache`, `offline_lookup`, `online_lookup`, `cache_precision`, `nominatim_url` und `accept_language` wirken wie bisher.
- **Namensnennung (CC BY 4.0):**
  - in `NOTICE`, in der README von `reversegeo` und im CHANGELOG, mit dem Datum der Daten;
  - die Nennung „Natural Earth“ entfällt aus `NOTICE` und aus den Log-Meldungen.
- **Dokumentation:**
  - README von `reversegeo`: §1, §2.2–2.5, Grenzfall „nächster Ort im Nachbarland“;
  - `docs/build-instructions.md` (`make geonames`);
  - `docs/arc42.md`;
  - CHANGELOG.

## Nicht-Ziele

- Keine Gemeinde- oder Landesgrenzen als Polygone. Zugeordnet wird der nächste Ort, nicht die Gemeinde, in der der Punkt liegt.
- Keine Änderung am Online-Weg. Dass Nominatim-Ergebnisse die englischen Werte in die `local_*`-Felder kopieren, wird hier nicht geändert.
- Kein persistenter KV-Speicher.
- Keine automatische oder zeitgesteuerte Aktualisierung der Daten.
- Weder die Dumps noch `alternateNamesV2` werden im Repository abgelegt.
- `Dockerfile` und `config.docker.yaml` werden nicht angepasst.

## Akzeptanzkriterien

- **AC-1** Offline (`online_lookup: false`) wird ein Foto in Freising, in Moosburg an der Isar oder in Garmisch-Partenkirchen diesem Ort zugeordnet. Dazu gehören Region `Bavaria` / `Bayern`, Land `Germany` / `Deutschland`, `country_code` `DE` und `local_language` `de`.
- **AC-2** Offline bekommen Punkte in Salzburg, Prag, Straßburg, Stettin und Basel einen Ort im richtigen Land (AT, CZ, FR, PL, CH). Prag heißt `Prague` / `Praha`, nicht wie ein Ortsteil.
- **AC-3** Offline bekommen Punkte in Chicago, Palermo und Bordeaux ihren eigenen Ort und ihre eigene Region, nicht die Hauptstadt des Landes.
- **AC-4** Liegt kein Ort innerhalb von `city_radius_km` (etwa mitten im Atlantik), bleiben die Namen offline leer. Die Koordinaten bleiben erhalten, das Plugin warnt, und es gibt keinen Cache-Eintrag. Mit `online_lookup: true` wird stattdessen online gefragt.
- **AC-5** `city_radius_km` gilt als Höchstentfernung zum nächsten Ort, mit Standard 25 und Bereich 1–200. Ein Punkt 30 km vom nächsten Ort bleibt mit dem Standard offline unaufgelöst und wird mit `city_radius_km: 50` aufgelöst.
- **AC-6** Das Plugin enthält keine fest eingebauten Orte oder Länder mehr. Jede Offline-Zuordnung stammt aus `places.bin`.
- **AC-7** Fehlt einem Ort ein Name in der Landessprache, nimmt `local_city` bzw. `local_region` den GeoNames-Hauptnamen desselben Orts bzw. derselben Verwaltungseinheit (Moosburg → `Moosburg`); hat er einen, gilt dieser (Munich → `München`). Fehlt der englische Name, gilt ebenfalls der GeoNames-Hauptname. In mehrsprachigen Ländern gilt die erste Landessprache (Genf → `local_city` `Genf`, `local_language` `de`). `local_country` ohne Namen in der Landessprache bleibt leer. Kein Name wird von einem anderen Ort übernommen.
- **AC-8** Das Werkzeug erzeugt aus denselben Eingaben byte-gleiche Ausgaben, verwirft `PPLX` und schreibt Formatversion und Datum in den Kopf. Das Plugin lehnt eine Datei mit unbekannter Formatversion beim Bauen oder in einem Test ab.
- **AC-9** `NOTICE`, die README von `reversegeo` und das CHANGELOG nennen GeoNames, CC BY 4.0 und das Datum der Daten. „Natural Earth“ steht weder in `NOTICE` noch in einer Log-Meldung.
- **AC-10** `places.bin` ist höchstens 8 MB groß. Eine Zuordnung braucht im Gast höchstens 5 ms. Ein Aufruf von `reversegeo` über den Host braucht im Release-Build ohne Netz höchstens 20 ms, einschließlich Instanziierung. Das Tech Spec nennt die gemessenen Werte.
