---
status: superseded
date: 2026-11-27
supersedes:
  - docs/adr/0005-geonames-gazetteer.md
references:
  - docs/adr/0002-plugins-without-engine-no-output.md
  - docs/adr/0003-wasm-component-execution.md
  - docs/adr/0004-component-cache.md
---

# ADR 0006 — Ortsnamen im GeoNames-Ortsverzeichnis: Hauptname als lokaler Rückfall

## Kontext

[ADR 0005](0005-geonames-gazetteer.md) führt für `reversegeo` ein eingebettetes Ortsverzeichnis aus GeoNames `cities1000` ein (Issue #64). Für die lokalen Namen legt es fest (Entscheidung 4): `local_city` und `local_region` sind der bevorzugte Name in der ersten Landessprache aus `countryInfo.txt`. Fehlt er, bleibt das Feld leer.

Beim Vorbereiten des Tech Spec wurde die Regel am 2026-11-27 an den Dumps vom selben Tag gemessen. Ausgewertet wurden 162.380 Orte, ohne Ortsteile (`PPLX`), sowie die Einträge aus `alternateNamesV2.txt` ohne historische und umgangssprachliche Namen. Ergebnis: Nur ein kleiner Teil der Orte hat überhaupt einen eigenen Namen in der Landessprache.

| Land |   Orte | mit Namen in der ersten Landessprache |
| :--- | -----: | ------------------------------------: |
| DE   |  7.280 |                                  20 % |
| AT   |  2.162 |                                  11 % |
| CH   |  1.195 |                                  34 % |
| FR   |  8.774 |                                  23 % |
| IT   |  9.922 |                                  60 % |
| CZ   |  1.478 |                                   9 % |
| PL   |  2.906 |                                  11 % |
| GB   |  4.469 |                                  12 % |
| US   | 16.665 |                                  68 % |
| JP   |  2.008 |                                  92 % |

Für die übrigen Orte führt GeoNames den amtlichen lokalen Namen als Hauptnamen (`name`), etwa „Moosburg“, „Mühlhausen“ oder „Köln“. Nur bei bekannten Städten ist der Hauptname englisch („Munich“, „Geneva“). Gerade diese Städte haben aber einen eigenen Eintrag in der Landessprache („München“). Mit ADR 0005 bliebe `local_city` für etwa 80 % der deutschen Orte leer, darunter Moosburg, obwohl der Datensatz den lokalen Namen enthält.

Ein zweiter Befund betrifft mehrsprachige Länder. `countryInfo.txt` nennt nur eine Reihenfolge der Sprachen: CH `de, fr, it, rm`; BE `nl, fr, de`; IT `it, de, fr, …`. Die GeoNames-Kennzeichnung „bevorzugt“ hilft nicht, die Sprache eines Orts zu bestimmen. Genève ist auf Französisch nicht als bevorzugt markiert. Bruxelles und Brussel sind beide markiert. Mit „erste Landessprache“ heißt Genf lokal „Genf“ (`de`), Brüssel „Brussel“ (`nl`).

## Varianten

### Lokaler Name ohne Eintrag in der Landessprache

| Variante                               | Vorteile                                                                                                                                                                                                        | Nachteile                                                                                                                                                                                                                  |
| :------------------------------------- | :-------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | :------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **A. GeoNames-Hauptname als Rückfall** | <ul><li>Jeder Ort hat einen lokalen Namen.</li><li>Der Name stammt aus demselben Datensatz und für denselben Ort; er ist nicht erfunden.</li><li>Er ist in den allermeisten Fällen der amtliche Name.</li></ul> | <ul><li>Bei Orten, deren Hauptname englisch ist und die keinen Eintrag in der Landessprache haben, steht ein englischer Name im lokalen Feld. Das ist selten, weil gerade solche Orte meist einen Eintrag haben.</li></ul> |
| **B. Leer lassen** (ADR 0005)          | <ul><li>Streng: Ein lokales Feld ist nur gefüllt, wenn die Sprache ausdrücklich belegt ist.</li></ul>                                                                                                           | <ul><li>Etwa 80 % der deutschen Orte (AT, CZ, PL rund 90 %) bekommen keinen lokalen Namen, obwohl der Datensatz ihn enthält.</li></ul>                                                                                     |

### Sprache in mehrsprachigen Ländern

| Variante                                                         | Vorteile                                                                                       | Nachteile                                                                                                                     |
| :--------------------------------------------------------------- | :--------------------------------------------------------------------------------------------- | :---------------------------------------------------------------------------------------------------------------------------- |
| **A. Erste Sprache aus `countryInfo.txt`, dokumentiert**         | <ul><li>Einfach und deterministisch.</li><li>`local_language` passt immer zum Namen.</li></ul> | <ul><li>Genf heißt lokal „Genf“, Brüssel „Brussel“, Bozen „Bolzano“; das entspricht nicht immer der Sprache am Ort.</li></ul> |
| **B. Sprache je Verwaltungseinheit** (gepflegte Tabelle)         | <ul><li>Genauer, etwa Kanton Genf → `fr`.</li></ul>                                            | <ul><li>Eine handgepflegte Tabelle im Projekt mit eigener Prüf- und Pflegelast.</li></ul>                                     |
| **C. Leer lassen, wenn sich die Namen je Sprache unterscheiden** | <ul><li>Nichts Unsicheres im lokalen Feld.</li></ul>                                           | <ul><li>Zürich, Genf und Brüssel hätten keinen lokalen Namen.</li></ul>                                                       |

## Entscheidung

Diese ADR ersetzt ADR 0005 vollständig. Die Entscheidungen 1–3, 5 und 6 übernimmt sie unverändert; Entscheidung 4 (Namen) ändert sie.

1. **Datensatz.** `reversegeo` nutzt GeoNames `cities1000`. Ortsteile (Feature-Code `PPLX`) werden beim Aufbereiten verworfen.
2. **Einbetten.** Die aufbereiteten Daten werden mit `include_bytes!` in die Komponente eingebettet. `reversegeo` bekommt keinen zusätzlichen Grant.
   - Das Format hat einen Kopf mit Formatversion, Quelle und Datum der Dumps.
   - Darauf folgen ein Gitterindex, die Einträge und eine Namenstabelle.
   - Das Format wird zur Laufzeit ohne Aufbau und ohne `unsafe` gelesen.
3. **Committete Binärdatei.** `make geonames` lädt die Dumps, bereitet sie mit einem Rust-Werkzeug im Workspace auf und schreibt `plugins/reversegeo/data/places.bin`.
   - Die Datei wird committet. Der normale Build braucht kein Netz.
   - Die Dumps selbst werden nicht committet.
   - Aktualisiert wird nur bewusst, mit eigenem Commit, der das Datum der Daten nennt.
4. **Namen** (geändert gegenüber ADR 0005).
   - **Landessprache:** Die Landessprache eines Orts ist die erste Sprache seines Landes aus `countryInfo.txt` (Variante A). Mehrsprachige Länder bekommen damit für alle Orte dieselbe Sprache. Die README von `reversegeo` nennt diese Einschränkung mit Beispielen.
   - **`city`, `region`, `county`:** der bevorzugte englische Name aus `alternateNamesV2`. Fehlt er, gilt der GeoNames-Hauptname (`name`, bzw. der Name aus `admin1CodesASCII`/`admin2Codes`).
   - **`local_city`, `local_region`:** der bevorzugte Name in der Landessprache. Fehlt er, gilt der GeoNames-Hauptname desselben Orts bzw. derselben Verwaltungseinheit (Variante A).
   - **`local_language`:** die Landessprache.
   - **`country`:** der Name aus `countryInfo.txt`.
   - **`local_country`:** der Name des Landes in der Landessprache aus `alternateNamesV2`, wobei Kurzformen Vorrang haben (z. B. „Deutschland“ statt „Bundesrepublik Deutschland“). Fehlt er, bleibt das Feld leer.
   - Es wird kein Name übersetzt, gebildet oder von einem anderen Ort übernommen.
5. **Zuordnung.**
   - Das Plugin wählt den nächsten Ort nach Großkreisentfernung innerhalb einer Höchstentfernung. Name und Standard dieser Einstellung legt das PRD fest.
   - Stadt, Kreis, Region und Land stammen von diesem Ort.
   - Ohne Ort innerhalb der Höchstentfernung fragt das Plugin online (wenn `online_lookup` an ist). Sonst bleiben die Namen leer, das Plugin warnt und schreibt nichts in den Cache (ADR 0002).
   - `CITIES` und `COUNTRY_BOXES` entfallen ersatzlos. Das behebt #62 und #63.
6. **Lizenz.** Die Daten stehen unter CC BY 4.0. `NOTICE`, die README von `reversegeo` und die Release Notes nennen GeoNames mit Lizenz und Datum der Daten. Die Nennung „Natural Earth“ für die bisherige Tabelle entfällt, aus `NOTICE` ebenso wie aus der Log-Meldung. Der Projektcode bleibt unter Apache-2.0.

Die Messwerte und Varianten zu den Entscheidungen 1–3 stehen in ADR 0005 und gelten weiter als Begründung.

## Konsequenzen

**Positiv**

- Jeder Ort hat einen lokalen Namen aus GeoNames; Moosburg heißt lokal „Moosburg“, München „München“.
- Die Regel bleibt deterministisch und ohne handgepflegte Tabellen.
- Alle positiven Folgen aus ADR 0005 gelten weiter.

**Negativ**

- In mehrsprachigen Ländern entspricht der lokale Name nicht immer der Sprache am Ort, z. B. „Genf“ statt „Genève“. Das ist dokumentiert, aber nicht behoben.
- Bei wenigen Orten mit englischem Hauptnamen und ohne Eintrag in der Landessprache steht ein englischer Name im lokalen Feld.
- Alle negativen Folgen aus ADR 0005 gelten weiter: Die Komponente wird größer, unter macOS werden die Daten je Instanziierung kopiert, jede Aktualisierung vergrößert die Git-Historie, und der nächste Ort kann in Grenznähe im Nachbarland liegen.

**Verfassung**

- Keine Regel ändert sich.
- _Strikte Regeln_ 2 bleibt gewahrt: Der Rückfall nutzt einen Namen desselben Orts aus derselben Quelle und erfindet nichts.
- ADR 0005 bekommt nach Annahme dieser ADR den Status `superseded`.
