---
status: superseded
date: 2026-11-27
supersedes: []
references:
  - docs/adr/0002-plugins-without-engine-no-output.md
  - docs/adr/0003-wasm-component-execution.md
  - docs/adr/0004-component-cache.md
---

# ADR 0005 — Offline-Ortsverzeichnis aus GeoNames für reversegeo

## Kontext

`reversegeo` löst GPS-Koordinaten offline über eine fest eingebaute Tabelle auf (`plugins/reversegeo/src/lib.rs`). Sie besteht aus zwei Teilen:

- 35 Referenzstädte weltweit (`CITIES`), davon 9 in Deutschland;
- 7 Länder-Rechtecke (`COUNTRY_BOXES`).

Issue #64 schlägt vor, ein großes Ortsverzeichnis einzubetten. Die Issues #62 und #63 belegen, dass die heutige Tabelle Daten erfindet. Das widerspricht ADR 0002 und _Strikte Regeln_ 2.

- **Länder-Rechteck (#62):** Jedes Rechteck liefert eine feste Hauptstadt als Stadt und Region. So wird Chicago zu „San Francisco“, Palermo zu „Rome“ und Bordeaux zu „Paris“.
- **Nächste Referenzstadt (#63):** Die nächste Referenzstadt im Umkreis von 200 km gilt als Ort des Fotos, mitsamt ihrem Land. So wird Garmisch-Partenkirchen zu „Innsbruck, Austria“, Prag zu „Dresden, Germany“ und Salzburg zu „Munich, Germany“.
- **Kleinstädte:** Fotos aus Freising oder Moosburg heißen „Munich“. Ein kleinerer Radius hilft nicht, er führt nur öfter in den Rückfall aus #62.

Der Online-Dienst Nominatim kennt die echten Orte. Er hat aber drei Nachteile:

- Er bekommt die Koordinaten jedes Fotos, was ein Datenschutzproblem ist.
- Er erlaubt nur etwa eine Anfrage pro Sekunde.
- Seine Ergebnisse überleben keinen Neustart, weil der KV-Speicher nur im Arbeitsspeicher liegt.

**GeoNames** ([geonames.org](https://www.geonames.org/), Lizenz [CC BY 4.0](https://creativecommons.org/licenses/by/4.0/)) bietet täglich aktualisierte Dumps besiedelter Orte (`cities<N>.zip`). Dazu gibt es Tabellen für Verwaltungsnamen (`admin1CodesASCII.txt`, `admin2Codes.txt`), Landesdaten (`countryInfo.txt`) und alternative Namen je Sprache (`alternateNamesV2.zip`, etwa 205 MB).

**Messung am 2026-11-27** (Dumps vom selben Tag):

- „Entfernung“ meint die Entfernung zum nächsten Ort. Gemessen wurde auf einem 0,1°-Raster über Deutschland und die Alpen (47,3–55,0° N, 6,0–15,0° O, 7.007 Punkte, inklusive Nord- und Ostsee).
- Ortsteile (`PPLX`) sind ausgenommen.
- Die Größe ist geschätzt, für ein kompaktes Binärformat mit Koordinaten als `f32`, Indizes und Namen.

| Datensatz     | Orte gesamt | davon DE |  Median |    90 % | Anteil über 10 km | Größe kompakt |
| :------------ | ----------: | -------: | ------: | ------: | ----------------: | ------------: |
| `cities15000` |      34.248 |    1.142 | 12,5 km | 36,0 km |              61 % |      ≈ 0,8 MB |
| `cities5000`  |      70.119 |    3.096 |  6,8 km | 19,1 km |              30 % |      ≈ 1,6 MB |
| `cities1000`  |     172.108 |    7.713 |  3,7 km | 10,0 km |              10 % |      ≈ 4,0 MB |
| `cities500`   |     237.408 |   12.018 |  3,0 km |  8,2 km |               8 % |      ≈ 5,5 MB |

Für alle Fälle aus #62/#63 liefert jeder Datensatz den richtigen Ort im richtigen Land, darunter Freising, Moosburg, Garmisch-Partenkirchen, Salzburg, Straßburg, Stettin, Basel und Palermo. Nimmt man Ortsteile (`PPLX`) mit, wird Prag zu „Královské Vinohrady“ und Chicago zu „Chicago Loop“.

Zwei Randbedingungen gelten für jede Lösung:

- **Ausführung:** Jede Komponente wird je Dokument in einem frischen `Store` instanziiert (ADR 0003). Ein Index im Gast überlebt also keinen Aufruf; er muss ohne Aufbau nutzbar sein. Unter macOS initialisiert wasmtime den Speicher nicht per Copy-on-Write. Das ginge nur mit `deserialize_file`, und das schließt ADR 0004 aus. Eingebettete Daten werden deshalb bei jeder Instanziierung kopiert.
- **Namen:** Der GeoNames-`name` ist gemischt, mal englisch, mal in der Landessprache: „Munich“, aber „Köln“. Die Namen aus `admin1` sind englisch, etwa „Bavaria“. Das Dokumentmodell kennt beide Sprachen (`city`/`local_city`, `region`/`local_region`).

Diese ADR klärt fünf Fragen:

1. Welcher Datensatz?
2. Wie kommen die Daten in das Plugin?
3. Wie kommen sie in das Repository und den Build?
4. Welche Namen werden geliefert?
5. Nach welcher Regel wird ein Ort zugeordnet, und was geschieht mit der alten Tabelle?

## Varianten

### Frage 1 — Datensatz

| Variante                                   | Vorteile                                                                                                   | Nachteile                                                                                             |
| :----------------------------------------- | :--------------------------------------------------------------------------------------------------------- | :---------------------------------------------------------------------------------------------------- |
| **A. `cities1000`**                        | <ul><li>Orte über 1.000 Einwohner und Verwaltungssitze.</li><li>Median 3,7 km, 90 % unter 10 km.</li></ul> | <ul><li>≈ 4 MB.</li></ul>                                                                             |
| **B. `cities500`**                         | <ul><li>Etwas dichter (Median 3,0 km).</li></ul>                                                           | <ul><li>38 % größer bei kleinem Gewinn.</li></ul>                                                     |
| **C. `cities5000`** / **D. `cities15000`** | <ul><li>Klein.</li></ul>                                                                                   | <ul><li>Kleinere Orte fehlen; 30 % bzw. 61 % der Punkte liegen über 10 km vom nächsten Ort.</li></ul> |

### Frage 2 — Auslieferung

| Variante                                              | Vorteile                                                                                                                                                                            | Nachteile                                                                                                                                                                                                                          |
| :---------------------------------------------------- | :---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | :--------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **A. In die Komponente einbetten** (`include_bytes!`) | <ul><li>Kein Grant und keine Datei zur Laufzeit.</li><li>Komponente und Daten passen immer zusammen.</li><li>Der Index lässt sich ohne Aufbau direkt aus den Bytes lesen.</li></ul> | <ul><li>Die Komponente wächst um ≈ 4 MB.</li><li>Unter macOS werden die Daten je Instanziierung kopiert (≈ 4 MB, Kosten im Tech Spec zu messen).</li><li>Neue Daten brauchen einen neuen Build der Komponente.</li></ul>           |
| **B. Datei über `fs-read`** (Setting `data_path`)     | <ul><li>Kleine Komponente.</li><li>Daten ohne Rebuild austauschbar.</li></ul>                                                                                                       | <ul><li>`reversegeo` braucht zusätzlich den Grant `filesystem`, das erweitert seine Rechte.</li><li>Jedes Foto liest ≈ 4 MB über die WIT-Grenze, langsamer als A.</li><li>Datei und Komponente können auseinanderlaufen.</li></ul> |
| **C. Neue Host-Capability** (`geo-lookup` in WIT)     | <ul><li>Der Index wird einmal geladen; am schnellsten.</li></ul>                                                                                                                    | <ul><li>Fachlogik wandert in den Host.</li><li>Neuer WIT-Vertrag und eine Schicht mehr.</li><li>Andere Plugins erhielten eine Geo-Funktion, die nicht ihre Sache ist.</li></ul>                                                    |

### Frage 3 — Daten im Repository

| Variante                                                               | Vorteile                                                                                                        | Nachteile                                                                                                   |
| :--------------------------------------------------------------------- | :-------------------------------------------------------------------------------------------------------------- | :---------------------------------------------------------------------------------------------------------- |
| **A. Aufbereitete Binärdatei committen** (`make geonames` erzeugt sie) | <ul><li>Der Build bleibt offline und reproduzierbar.</li><li>Die Datenversion ist im Commit sichtbar.</li></ul> | <ul><li>Jede Aktualisierung kostet ≈ 4 MB Git-Historie.</li></ul>                                           |
| **B. Beim Build herunterladen**                                        | <ul><li>Kleines Repository.</li></ul>                                                                           | <ul><li>Der Build braucht Netz.</li><li>Nicht reproduzierbar, weil sich die Dumps täglich ändern.</li></ul> |
| **C. Git LFS**                                                         | <ul><li>Kleine Historie.</li></ul>                                                                              | <ul><li>Zusätzliche Einrichtung.</li><li>Begrenztes LFS-Kontingent.</li></ul>                               |

### Frage 4 — Namen

| Variante                                                 | Vorteile                                                                                                                 | Nachteile                                                                                                   |
| :------------------------------------------------------- | :----------------------------------------------------------------------------------------------------------------------- | :---------------------------------------------------------------------------------------------------------- |
| **A. Englisch und Landessprache** aus `alternateNamesV2` | <ul><li>Füllt `city`/`local_city` und `region`/`local_region` einheitlich, wie das Dokumentmodell es vorsieht.</li></ul> | <ul><li>Die Aufbereitung braucht den großen Dump (≈ 205 MB). Datei und Laufzeit ändern sich kaum.</li></ul> |
| **B. Nur der GeoNames-`name`**                           | <ul><li>Einfach.</li></ul>                                                                                               | <ul><li>Die Sprache ist uneinheitlich.</li><li>Die `local_*`-Felder bleiben offline leer.</li></ul>         |

### Frage 5 — Zuordnung und alte Tabelle

Der nächste Ort aus einem dichten Verzeichnis ist mit hoher Wahrscheinlichkeit der Ort selbst oder ein direkter Nachbar. Land und Region dieses Orts gehören zum Ort, nicht zu einer fernen Referenzstadt. Dafür muss eine Höchstentfernung gelten, damit offene See oder dünn besiedelte Gebiete keinen weit entfernten Ort bekommen. Länder-Rechtecke und Referenzstädte werden dann nicht mehr gebraucht; sie sind die Ursache von #62 und #63.

## Entscheidung

1. **Datensatz (1A).** `reversegeo` nutzt GeoNames `cities1000`. Ortsteile (Feature-Code `PPLX`) werden beim Aufbereiten verworfen.
2. **Einbetten (2A).** Die aufbereiteten Daten werden mit `include_bytes!` in die Komponente eingebettet. `reversegeo` bekommt keinen zusätzlichen Grant.
   - Das Format hat einen Kopf mit Formatversion, Quelle und Datum der Dumps.
   - Darauf folgen ein Gitterindex, die Einträge und eine Namenstabelle.
   - Das Format wird zur Laufzeit ohne Aufbau und ohne `unsafe` gelesen (`from_le_bytes` auf Teilbereichen). So bleibt es mit der Instanziierung je Dokument vereinbar.
3. **Committete Binärdatei (3A).** `make geonames` lädt die Dumps, bereitet sie mit einem Rust-Werkzeug im Workspace auf und schreibt `plugins/reversegeo/data/places.bin`.
   - Die Datei wird committet. Der normale Build braucht kein Netz.
   - Die Dumps selbst werden nicht committet.
   - Aktualisiert wird nur bewusst, durch einen Lauf von `make geonames`. Die neue Datei kommt mit eigenem Commit, der das Datum nennt.
4. **Namen (4A).**
   - **`city`, `region`, `county`:** der bevorzugte englische Name aus `alternateNamesV2`. Fehlt er, gilt der GeoNames-Hauptname (`name`, bzw. der Name aus `admin1CodesASCII`/`admin2Codes`).
   - **`local_city`, `local_region`:** der bevorzugte Name in der ersten Landessprache aus `countryInfo.txt`. Fehlt er, bleibt das Feld leer.
   - **`local_language`:** diese Landessprache.
   - **`country`/`local_country`:** analog aus `countryInfo.txt` und den alternativen Namen des Landes.
   - Es wird kein Name übersetzt oder aus einem anderen Ort übernommen.
5. **Zuordnung.**
   - Das Plugin wählt den nächsten Ort nach Großkreisentfernung, sofern er innerhalb einer Höchstentfernung liegt. Name und Standard dieser Einstellung legt das PRD fest.
   - Stadt, Kreis, Region und Land stammen von diesem Ort.
   - Liegt kein Ort innerhalb der Höchstentfernung, fragt das Plugin online (wenn `online_lookup` an ist). Sonst bleiben die Namen leer, das Plugin warnt und schreibt nichts in den Cache (ADR 0002).
   - `CITIES` und `COUNTRY_BOXES` entfallen ersatzlos. Das behebt #62 und #63.
6. **Lizenz.** Die Daten stehen unter CC BY 4.0. `NOTICE`, die README von `reversegeo` und die Release Notes nennen GeoNames mit Lizenz und Datum der Daten. Die irreführende Nennung „Natural Earth“ für die bisherige Tabelle entfällt, aus `NOTICE` ebenso wie aus der Log-Meldung. Der Projektcode bleibt unter Apache-2.0; die Daten sind kein Code.

Die Umsetzung folgt als Feature mit PRD und Tech Spec. Das Tech Spec legt das Binärformat fest und misst Größe, Ladezeit und Kosten je Instanziierung.

## Konsequenzen

**Positiv**

- Fotos aus Kleinstädten bekommen offline ihren echten Ort, ihre Region und ihr Land, ohne dass Koordinaten das Gerät verlassen.
- Die erfundenen Orte aus #62 und #63 verschwinden mit der alten Tabelle.
- Die Ergebnisse sind deterministisch und hängen nicht vom Netz oder vom Neustart des KV-Speichers ab.
- Nominatim wird nur noch für Punkte fern jedes Orts gebraucht.

**Negativ**

- Die Komponente wird um ≈ 4 MB größer. Das Kompilieren beim ersten Start dauert länger; der Komponenten-Cache (ADR 0004) fängt das danach ab.
- Unter macOS kostet jede Instanziierung eine Kopie der eingebetteten Daten. Das Tech Spec muss die Kosten messen und begrenzen.
- Jede Datenaktualisierung vergrößert die Git-Historie um ≈ 4 MB.
- Die Aufbereitung braucht einmalig ≈ 250 MB Downloads und ein zusätzliches Werkzeug im Workspace.
- Der nächste Ort ist nicht immer die Gemeinde, in der das Foto entstand. In Grenznähe kann er im Nachbarland liegen, wenn dort der nächste Ort liegt. Das ist bei 3,7 km Median selten, aber möglich; das Plugin-README nennt es.
- Die bisherige Bedeutung von `city_radius_km` (Radius um 35 Referenzstädte) entfällt. Das PRD regelt die Ablösung.

**Verfassung**

- Keine Regel ändert sich.
- _Strikte Regeln_ 1 und 2 bleiben gewahrt: Es gibt kein `unsafe`, und das Plugin erfindet keine Orte mehr.
- _Security_ 1–2 bleiben gewahrt: `reversegeo` braucht keinen neuen Grant.
