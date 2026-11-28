---
status: accepted
date: 2026-11-28
supersedes: []
references:
  - docs/adr/0002-plugins-without-engine-no-output.md
  - docs/adr/0003-wasm-component-execution.md
  - docs/adr/0011-working-image-pure-rust-decoders.md
---

# ADR 0012 — Thumbnails nur aus geschwärzten Bildern, als AVIF

## Kontext

Seit 0.4.0 erkennt `face-detection` echte Gesichter, und `privacy-redact` schreibt eine geschwärzte Kopie. Die Bilder, die Web-Oberfläche und API zeigen, sind aber **nicht geschwärzt** (Issue #76):

- **Reihenfolge:** `thumbnail` läuft als Stufe 5, `face-detection` und `privacy-redact` erst als Stufen 11 und 12. Die Thumbnails entstehen also, bevor ein Gesicht bekannt ist.
- **Ablage:** Die geschwärzte Kopie liegt in `cache/redacted/<id>.redacted.jpg`. Kein Teil, der Bilder ausliefert, nutzt sie.
- **Rückfälle im Server:**
  - `GET /api/photos/:id/thumbnail/:size` liefert das **Original in voller Auflösung**, wenn die Thumbnail-Datei fehlt;
  - gibt es auch das Original nicht, kommt ein eingebautes 1×1-JPEG, entgegen ADR 0002;
  - beides geht mit `Cache-Control: public, max-age=86400` hinaus.

Mit dem Standard `redact_faces: true` bleiben Gesichter damit überall sichtbar, wo die Oberfläche ein Foto zeigt.

Außerdem wünscht der Maintainer **Thumbnails im Format AVIF** (2026-11-28). ADR 0011 hat `oxideav-avif` 0.0.18 (MIT, kein `unsafe`) als Decoder festgelegt; die Crate kodiert auch.

**Messung vom 2026-11-28** (Release-Build, `wasm32-wasip2` mit SIMD, ein Thread, macOS, Apple Silicon). Quelle ist `apollo11.jpg` (1600 × 1257), verkleinert mit Lanczos3. Der PSNR bezieht sich auf das verkleinerte Bild.

| Längste Seite | AVIF q70 (`EncodeSpeed::Fast`)                         | JPEG q75                  | JPEG q85                  |
| ------------: | :----------------------------------------------------- | :------------------------ | :------------------------ |
|        128 px | 81 ms, 4.947 B, 36,5 dB                                | < 1 ms, 4.777 B, 31,3 dB  | < 1 ms, 6.196 B, 33,7 dB  |
|        256 px | 324 ms, 10.954 B, 29,1 dB                              | 1 ms, 13.583 B, 32,8 dB   | 1 ms, 18.010 B, 35,1 dB   |
|        512 px | 668 ms, 24.893 B, 30,7 dB                              | 4 ms, 41.097 B, 34,7 dB   | 4 ms, 54.874 B, 37,0 dB   |
|       1024 px | 1.441 ms, 75.151 B, 34,9 dB                            | 15 ms, 123.503 B, 37,4 dB | 17 ms, 166.439 B, 39,5 dB |
|       2048 px | 17.522 ms (Kacheln 1024: 9.403 ms), 205.412 B, 40,9 dB | 59 ms, 364.538 B, 41,6 dB | 64 ms, 493.963 B, 43,7 dB |

- **Größe:** AVIF q70 ist etwa 40–55 % kleiner als JPEG q75, bei 1–4 dB weniger PSNR. PSNR bewertet die weicheren AVIF-Artefakte eher zu schlecht.
- **Zeit:** Die Standardgrößen 128/512/1024 px kosten mit AVIF ≈ 2,2 s je Foto statt ≈ 0,02 s. 2048 px dauern ohne Kacheln 17,5 s, mit Kacheln von 1024 px 9,4 s (nativ 5,8 s bzw. 3,8 s).
- **Speicher:** 1024 px und 2048 px mit Kacheln passen in 256 MB Gastspeicher.
- **Wiedergabe:** Alle aktuellen Browser (Chrome, Edge, Firefox, Safari ab 16.4) zeigen AVIF an.

## Varianten

### Wie werden Thumbnails geschwärzt?

| Variante                                                                                                      | Vorteile                                                                                                                                                 | Nachteile                                                                                                                                                          |
| :------------------------------------------------------------------------------------------------------------ | :------------------------------------------------------------------------------------------------------------------------------------------------------- | :----------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **A. Reihenfolge: `face-detection` → `privacy-redact` → `thumbnail`; `thumbnail` liest das geschwärzte Bild** | <ul><li>Klare Zuständigkeit: Schwärzen in einem Plugin, Verkleinern in einem anderen.</li><li>Kein doppelter Code für Größen, Qualität, Namen.</li></ul> | <ul><li>Die Reihenfolge wird zur Regel; der Host muss sie prüfen.</li><li>`thumbnail` muss wissen, ob geschwärzt wurde, ohne `privacy-redact` zu kennen.</li></ul> |
| B. `privacy-redact` erzeugt die Thumbnails nach dem Schwärzen neu                                             | <ul><li>Reihenfolge bleibt.</li></ul>                                                                                                                    | <ul><li>`privacy-redact` übernimmt Thumbnail-Logik (Größen, Format, AVIF-Encoder).</li><li>Jedes Foto wird doppelt verkleinert.</li></ul>                          |

### Was gilt, wenn Schwärzen nicht möglich ist?

| Variante                                      | Vorteile                                                                                                                          | Nachteile                                                                                         |
| :-------------------------------------------- | :-------------------------------------------------------------------------------------------------------------------------------- | :------------------------------------------------------------------------------------------------ |
| **A. Fail closed: keine Thumbnails, Warnung** | <ul><li>Kein ungeschwärztes Gesicht wird angezeigt.</li><li>Passt zu ADR 0002: nichts erfinden, nichts Falsches zeigen.</li></ul> | <ul><li>Fotos ohne Gesichtsergebnis (z. B. nicht dekodierbar) erscheinen ohne Vorschau.</li></ul> |
| B. Thumbnails aus dem Original, Warnung       | <ul><li>Jedes Foto hat eine Vorschau.</li></ul>                                                                                   | <ul><li>Genau die Lücke aus Issue #76.</li></ul>                                                  |

### Format der Thumbnails

| Variante                                            | Vorteile                                                                                                                                                                | Nachteile                                                                                                    |
| :-------------------------------------------------- | :---------------------------------------------------------------------------------------------------------------------------------------------------------------------- | :----------------------------------------------------------------------------------------------------------- |
| **A. AVIF als Standard, JPEG per Setting `format`** | <ul><li>Etwa halb so große Dateien, weniger Speicher und schnellere Seiten.</li><li>Wunsch des Maintainers.</li><li>JPEG bleibt für schwache Rechner wählbar.</li></ul> | <ul><li>≈ 2,2 s mehr je Foto für die Standardgrößen; 2048 px ≈ 9,4 s.</li><li>Junge Encoder-Crate.</li></ul> |
| B. Nur AVIF                                         | <ul><li>Einfach.</li></ul>                                                                                                                                              | <ul><li>Kein Ausweg bei langsamer Hardware oder Fehlern im Encoder.</li></ul>                                |
| C. Beide Formate schreiben                          | <ul><li>Fallback für alte Clients.</li></ul>                                                                                                                            | <ul><li>Doppelter Platz, längste Laufzeit; alte Clients sind kein Ziel.</li></ul>                            |

## Entscheidung

1. **Reihenfolge (Variante A).** In jeder Pipeline, in der `privacy-redact` aktiv ist und `redact_faces` gilt, läuft `face-detection` vor `privacy-redact` und `privacy-redact` vor `thumbnail`. `thumbnail` steht in den mitgelieferten Konfigurationen und in `AppConfig::default()` am Ende.
2. **Prüfung durch den Host.** Beim Laden der Konfiguration lehnt der Host eine Pipeline ab, die diese Reihenfolge verletzt, oder in der `privacy-redact` mit `redact_faces` aktiv ist und `face-detection` fehlt oder abgeschaltet ist. Die Fehlermeldung nennt Plugin und Position. Ist `privacy-redact` aus oder `redact_faces: false`, gibt es keine Bedingung.
3. **Schwärzungsstand im Dokument.** `PrivacyInfo` erhält das Feld `face_redaction` mit den Werten `applied`, `no_faces`, `disabled` und `failed`. `privacy-redact` setzt es immer, sobald es läuft:
   - **`applied`:** Gesichter erkannt und geschwärzt, `redacted_image_path` zeigt auf das geschwärzte Bild in voller Größe;
   - **`no_faces`:** Gesichtsergebnis vorhanden, aber leer;
   - **`disabled`:** `redact_faces: false`;
   - **`failed`:** Schwärzen war verlangt, ging aber nicht (kein Gesichtsergebnis, Bild nicht lesbar, Fehler beim Schreiben).
4. **Quelle der Thumbnails.** Eine SDK-Funktion `display_source(&doc)` bestimmt sie für jedes Plugin, das Bilder zum Anzeigen erzeugt:
   - `applied`: `redacted_image_path`;
   - `no_faces`, `disabled` oder kein `doc.privacy`: das Arbeitsbild (`pixel_source`, ADR 0011);
   - `failed`: **keine Quelle**.
5. **Fail closed (Variante A).** Bei `failed` schreibt `thumbnail` keine Thumbnails, lässt `doc.thumbnails` leer und warnt mit Pfad und Grund. Es weicht nie auf das Original oder das ungeschwärzte Arbeitsbild aus. Das gilt für alle Plugins, die Anzeigebilder erzeugen.
6. **Ablage.** `privacy-redact` schreibt das geschwärzte Bild standardmäßig nach `cache/thumbnails/<id>.redacted.jpg` (Setting `output_dir`). `cache/redacted` entfällt.
7. **Server.**
   - `GET /api/photos/:id/thumbnail/:size` liefert nur Thumbnail-Dateien aus `doc.thumbnails`. Fehlt die Datei oder gibt es keine Thumbnails, antwortet er mit `404`. Es gibt keinen Rückfall auf das Original und kein Ersatzbild;
   - der `Content-Type` folgt der Datei (`image/avif` oder `image/jpeg`);
   - `Cache-Control` ist `private, max-age=86400`.
8. **AVIF-Thumbnails (Variante A).**
   - `thumbnail` kodiert mit `oxideav-avif` (`=0.0.18`, wie in ADR 0011), `EncodeSpeed::Fast`, ein Thread, 4:2:0, Kacheln von höchstens 1024 px;
   - Setting `format` mit `avif` (Standard) oder `jpeg`;
   - `quality` gilt für beide Formate, Standard 70 für AVIF und 75 für JPEG;
   - Dateiname `<key>_<size>.avif` bzw. `.jpg`. Ältere JPEG-Thumbnails bleiben gültig, bis das Foto neu verarbeitet wird.
9. **Grenzen.** Die Profile setzen `format` und `resource_limits` von `thumbnail` passend zur Hardware. Die Werte legt das PRD fest.

## Konsequenzen

**Positiv**

- Mit `redact_faces: true` zeigt keine Vorschau ein ungeschwärztes Gesicht, auch nicht über den Server-Rückfall oder den Browser-Cache.
- Die Regel „fail closed“ gilt für alle Plugins, die Anzeigebilder erzeugen, und ist im Dokument nachvollziehbar (`face_redaction`).
- Eine falsche Reihenfolge fällt beim Start auf, nicht erst durch ein sichtbares Gesicht.
- Thumbnails sind etwa halb so groß; die Oberfläche lädt schneller.

**Negativ**

- Etwa 2,2 s mehr Rechenzeit je Foto für die Standardgrößen, mit 2048 px etwa 9,4 s mehr. Auf schwachen Rechnern kann `format: jpeg` nötig sein.
- Fotos, bei denen das Schwärzen scheitert, erscheinen ohne Vorschau.
- Konfigurationen mit eigener Reihenfolge, die `thumbnail` vor `privacy-redact` stellen, starten nicht mehr und müssen angepasst werden.
- `PrivacyInfo` und damit WIT ändern sich; alle Komponenten werden neu gebaut.
- `oxideav-avif` wird jetzt auch als Encoder genutzt; dessen Reife ist ebenso jung wie die des Decoders (ADR 0011).
- Gespeicherte Dokumente älterer Läufe haben ungeschwärzte JPEG-Thumbnails, bis sie neu verarbeitet werden.

**Verfassung**

- Keine Regel ändert sich.
- _Architektur_ 3 bleibt gewahrt: `thumbnail` kennt `privacy-redact` nicht, sondern liest nur `doc.privacy` aus dem `UnifiedDocument` (_Architektur_ 4). Die Reihenfolge prüft der Host.
- _Strikte Regeln_ 2: kein Ersatzbild im Server, kein Rückfall auf ungeschwärzte Pixel.
- _Security_: Der Server gibt nur abgeleitete, bei Bedarf geschwärzte Dateien heraus.
