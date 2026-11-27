---
status: accepted
date: 2026-11-27
supersedes:
  - docs/adr/0006-geonames-gazetteer-names.md
references:
  - docs/adr/0002-plugins-without-engine-no-output.md
  - docs/adr/0003-wasm-component-execution.md
  - docs/adr/0004-component-cache.md
  - docs/adr/0005-geonames-gazetteer.md
---

# ADR 0008 — Kurze Regionsnamen im GeoNames-Ortsverzeichnis

## Kontext

[ADR 0006](0006-geonames-gazetteer-names.md) legt fest, wie `reversegeo` die Namen aus GeoNames wählt. Für Regionen (`region`, `local_region`) nimmt es den bevorzugten Namen (`isPreferredName`) in der jeweiligen Sprache. Das ist oft die lange amtliche Form, etwa „State of Salzburg“, „Canton of Geneva“, „Free and Hanseatic City of Hamburg“, „Land Berlin“ oder „Kanton Basel-Stadt“. Issue #67 fordert kürzere Namen; sie sind in Datenbank, Suche, Web-Oberfläche und Ordnernamen besser lesbar.

GeoNames markiert manche Namen als Kurzform (`isShortName`). Ausgewertet wurde am 2026-11-27 an den Dumps, aus denen `places.bin` gebaut ist: die 3.775 Regionen (admin1), auf die mindestens ein Ort verweist. Drei Regeln wurden für die englischen Namen verglichen:

| Regel                                                             | Geänderte Regionen | Beispiele                                                                                                                                                           | Ausreißer                                                                                                                                                                                |
| :---------------------------------------------------------------- | -----------------: | :------------------------------------------------------------------------------------------------------------------------------------------------------------------ | :--------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **A. Kurzform, wenn wörtlich enthalten**                          |                870 | State of Salzburg → Salzburg, Canton of Geneva → Geneva, Free and Hanseatic City of Hamburg → Hamburg, Île-de-France Region → Île-de-France, State of Maine → Maine | selten Adjektive (Masovian Voivodeship → Masovian); manches bleibt lang (Walloon Region, Canton of St. Gallen)                                                                           |
| **B. Kurzform immer**                                             |                949 | wie A, dazu Walloon Region → Wallonia, La Habana Province → Havana                                                                                                  | etwa 75 fragwürdige Fälle, z. B. Rhineland-Palatinate → Rheinland-Pfalz (deutscher Name im englischen Feld), Entre Ríos Province → Entre Rios (Akzent verloren), Pärnu County → Pärnumaa |
| **C. Kürzester enthaltener Name** (auch ohne Kurzform-Markierung) |                896 | wie A, dazu Canton of St. Gallen → St. Gallen                                                                                                                       | häufiger Adjektive (West Pomeranian, Masovian)                                                                                                                                           |

„Wörtlich enthalten“ heißt: Die Kurzform kommt im bevorzugten Namen als ganze Wortfolge vor. Davor steht der Anfang, ein Leerzeichen, ein Bindestrich, ein Apostroph oder eine öffnende Klammer, danach das Ende, ein Leerzeichen, ein Bindestrich, ein Apostroph, eine schließende Klammer oder ein Komma; Groß- und Kleinschreibung zählen. Die Prüfung bildet keinen Namen. Sie entscheidet nur, ob ein vorhandener GeoNames-Name übernommen wird.

**In der Landessprache** ändert Regel A 333 Regionen. Beispiele: Land Berlin → Berlin, Freie und Hansestadt Hamburg → Hamburg, Land Salzburg → Salzburg, Bundesland Wien → Wien, Kanton Basel-Stadt → Basel-Stadt, Kanton Genf → Genf, Région Île-de-France → Île-de-France, Hlavní město Praha → Praha, Comunidad Autónoma de Madrid → Madrid.

**Berlin:** GeoNames kennt auf Englisch nur „State of Berlin“, ohne Kurzform; auch `admin1CodesASCII` nennt „State of Berlin“. Auf Deutsch gibt es die Kurzform „Berlin“.

**Kreise (admin2):** Regel A würde 634 Kreise ändern, aber teils falsch. „Brighton and Hove“ würde zu „Brighton“, obwohl der Kreis beide Städte umfasst; „Český Krumlov District“ hat eine Kurzform ohne Diakritika.

## Varianten

| Frage               | Varianten                                                               | Bewertung                                                                                          |
| :------------------ | :---------------------------------------------------------------------- | :------------------------------------------------------------------------------------------------- |
| Regel für Regionen  | A / B / C (Tabelle oben)                                                | A ändert fast so viel wie B, ohne dessen Ausreißer.                                                |
| Berlin auf Englisch | „State of Berlin“ lassen / handgepflegte Liste (z. B. `DE.16 → Berlin`) | Eine Liste wäre genauer, wäre aber eine zweite, handgepflegte Datenquelle mit eigener Prüfpflicht. |
| Umfang              | nur `region` / `region` + `local_region` / zusätzlich `county`          | Bei Kreisen greift die Regel daneben; bei lokalen Regionsnamen wirkt sie wie bei englischen.       |

## Entscheidung

Diese ADR ersetzt ADR 0006 vollständig. Die Entscheidungen 1–3, 5 und 6 übernimmt sie unverändert. In Entscheidung 4 ändert sie nur die Regel für `region` und `local_region`.

1. **Datensatz.** `reversegeo` nutzt GeoNames `cities1000`. Ortsteile (Feature-Code `PPLX`) werden beim Aufbereiten verworfen.
2. **Einbetten.** Die aufbereiteten Daten werden mit `include_bytes!` in die Komponente eingebettet. `reversegeo` bekommt keinen zusätzlichen Grant.
   - Das Format hat einen Kopf mit Formatversion, Quelle und Datum der Dumps.
   - Darauf folgen ein Gitterindex, die Einträge und eine Namenstabelle.
   - Das Format wird zur Laufzeit ohne Aufbau und ohne `unsafe` gelesen.
3. **Committete Binärdatei.** `make geonames` lädt die Dumps, bereitet sie mit `tools/geonames-import` auf und schreibt `plugins/reversegeo/data/places.bin`.
   - Die Datei wird committet; der normale Build braucht kein Netz.
   - Die Dumps werden nicht committet.
   - Aktualisiert wird nur bewusst, mit eigenem Commit, der das Datum der Daten nennt.
4. **Namen.**
   - **Landessprache:** die erste Sprache des Landes aus `countryInfo.txt`. Mehrsprachige Länder bekommen damit für alle Orte dieselbe Sprache; die README von `reversegeo` nennt das mit Beispielen.
   - **`city`:** der bevorzugte englische Name aus `alternateNamesV2`, sonst der GeoNames-Hauptname.
   - **`local_city`:** der bevorzugte Name in der Landessprache, sonst der GeoNames-Hauptname.
   - **`county`:** der bevorzugte englische Name, sonst der Name aus `admin2Codes`. Unverändert, die neue Regel gilt hier nicht.
   - **`region` und `local_region`** (neu, Regel A): Zuerst wird der Grundname bestimmt, wie bisher: der bevorzugte Name in der Sprache (Englisch bzw. Landessprache), sonst der Name aus `admin1CodesASCII`. Gibt es in derselben Sprache einen als Kurzform markierten Namen, der im Grundnamen wörtlich enthalten ist (Definition im Kontext), gilt die Kurzform. Bei mehreren gilt die mit der kleinsten `alternateNameId`. Sonst bleibt der Grundname.
   - **Berlin und ähnliche Fälle:** Hat eine Region auf Englisch keine passende Kurzform, bleibt der Grundname, zum Beispiel „State of Berlin“. Es gibt keine handgepflegten Namen. Die README nennt Berlin als Beispiel.
   - **`local_language`:** die Landessprache.
   - **`country`:** der Name aus `countryInfo.txt`.
   - **`local_country`:** der Name des Landes in der Landessprache, wobei Kurzformen Vorrang haben. Fehlt er, bleibt das Feld leer.
   - Es wird kein Name übersetzt, gebildet, gekürzt oder von einem anderen Ort übernommen.
5. **Zuordnung.**
   - Das Plugin wählt den nächsten Ort nach Großkreisentfernung innerhalb von `city_radius_km` (Standard 25 km, Bereich 1–200).
   - Stadt, Kreis, Region und Land stammen von diesem Ort.
   - Liegt kein Ort innerhalb der Höchstentfernung, fragt das Plugin online (wenn `online_lookup` an ist). Sonst bleiben die Namen leer, das Plugin warnt und schreibt nichts in den Cache (ADR 0002).
   - Es gibt keine fest eingebauten Orte oder Länder-Rechtecke.
6. **Lizenz.** Die Daten stehen unter CC BY 4.0. `NOTICE`, die README von `reversegeo` und die Release Notes nennen GeoNames mit Lizenz und Datum der Daten. Der Projektcode bleibt unter Apache-2.0.

Die Begründungen und Messwerte zu den Entscheidungen 1–3 stehen in ADR 0005, die zu den übrigen Namensregeln in ADR 0006. Sie gelten weiter.

## Konsequenzen

**Positiv**

- 870 englische und 333 lokale Regionsnamen werden kürzer, ohne dass ein Name gebildet wird. Beispiele: Salzburg, Geneva, Hamburg, Île-de-France, Basel-Stadt, Praha.
- Die Regel bleibt deterministisch und rein datengetrieben, und die Prüfung „wörtlich enthalten“ hält offensichtliche Ausreißer fern.

**Negativ**

- Ein Teil der Regionen bleibt lang, weil GeoNames keine passende Kurzform führt. Dazu gehören „State of Berlin“, „Walloon Region“, „Canton of St. Gallen“ und die meisten polnischen Woiwodschaften.
- Vereinzelt wird ein Adjektiv übernommen, das GeoNames als Kurzform führt, etwa „Masovian“.
- `places.bin` muss neu erzeugt werden. Gespeicherte Dokumente behalten ihre alten Namen, bis ihre Fotos erneut verarbeitet werden.

**Verfassung**

- Keine Regel ändert sich. _Strikte Regeln_ 2 bleibt gewahrt: Jeder Name stammt unverändert aus GeoNames.
- ADR 0006 bekommt nach Annahme dieser ADR den Status `superseded`.
