---
status: accepted
date: 2026-11-27
supersedes: []
references:
  - docs/adr/0003-wasm-component-execution.md
---

# ADR 0004 — Cache für kompilierte Wasm-Komponenten

## Kontext

Seit [ADR 0003](0003-wasm-component-execution.md) lädt der Host jede aktivierte Komponente mit `Component::from_file`. Cranelift übersetzt sie dabei bei jedem Start neu in Maschinencode. ADR 0003 nennt einen Cache für kompilierte Komponenten ausdrücklich als spätere Erweiterung.

Messung am 2026-11-27 (Release-Build, macOS, 8 aktivierte Plugins, `flagme-host process Examples/sample.jpg`):

- Der Aufruf dauert insgesamt etwa 0,8 s.
- Davon entfallen etwa 0,6 s auf das Laden und Kompilieren der Komponenten.
- Alle Plugins zusammen brauchen nur etwa 0,1 bis 0,2 s; den größten Teil davon wartet `weather-enrich` auf das Netz.

Bei `process` und `stream`, die für jede Datei neu gestartet werden, macht das Kompilieren damit den größten Teil der Laufzeit aus. Bei `watch` und `serve` fällt es nur einmal beim Start an.

Für einen Cache gelten zwei Randbedingungen:

- **_Strikte Regeln_ 1:** Der Produktionscode ist sicheres, idiomatisches Rust. Die öffentlichen Ladefunktionen für vorkompilierten Code (`Component::deserialize`, `deserialize_file`) sind `unsafe`, weil wasmtime den gelesenen Maschinencode nicht prüfen kann.
- **_Security_ 1–3:** Wer Dateien in den Cache schreiben kann, kann nativen Code in den Host-Prozess einschleusen. Ein solcher Code läuft außerhalb der Sandbox und umgeht damit jede Capability-Prüfung. Der Cache-Ordner ist deshalb eine Vertrauensgrenze.

Diese ADR klärt drei Fragen:

1. Welcher Mechanismus speichert und lädt den kompilierten Code?
2. Wo liegt der Cache, und ist er standardmäßig an?
3. Was geschieht, wenn der Cache-Ordner unsicher oder nicht nutzbar ist?

## Varianten

### Frage 1 — Mechanismus

| Variante                                                                 | Vorteile                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                   | Nachteile                                                                                                                                                                                                                                                                                                |
| :----------------------------------------------------------------------- | :----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | :------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **A. Eingebauter Cache von wasmtime** (`Config::cache`, Feature `cache`) | <ul><li>Sichere API; der Host-Code braucht kein `unsafe`.</li><li>Der Schlüssel enthält einen Hash der Wasm-Bytes, die Engine-Konfiguration und die Compiler-Version. Eine geänderte Komponente, eine neue wasmtime-Version oder andere Engine-Einstellungen führen deshalb zu einem neuen Eintrag, nie zu veraltetem Code.</li><li>Gilt auch für Komponenten (`build_component_artifacts`).</li><li>Aufräumen nach Größe und Anzahl ist eingebaut.</li><li>Das Feature ist in der eingebundenen wasmtime-Version bereits aktiv.</li></ul> | <ul><li>Ablage, Format und Aufräumen liegen bei wasmtime; ein Hintergrund-Thread räumt auf.</li><li>Die Cache-Dateien sind nicht signiert. Die Sicherheit hängt an den Rechten des Ordners.</li></ul>                                                                                                    |
| **B. Eigener Cache mit `Component::serialize`/`deserialize_file`**       | <ul><li>Volle Kontrolle über Schlüssel, Ablage und Aufräumen.</li></ul>                                                                                                                                                                                                                                                                                                                                                                                                                                                                    | <ul><li>Braucht `unsafe` im Host. Das widerspricht _Strikte Regeln_ 1 und bräuchte eine Verfassungsänderung.</li><li>Schlüssel und Invalidierung müssten selbst gebaut und getestet werden; ein Fehler darin lädt veralteten Code.</li></ul>                                                             |
| **C. Vorkompilieren beim Build** (`wasmtime compile` → `.cwasm`)         | <ul><li>Kein Kompilieren zur Laufzeit, auch nicht beim ersten Start.</li></ul>                                                                                                                                                                                                                                                                                                                                                                                                                                                             | <ul><li>Laden braucht ebenfalls `unsafe` (`deserialize_file`).</li><li>Die Artefakte sind an CPU und wasmtime-Version gebunden; `dist/` müsste je Zielplattform passende Dateien liefern.</li><li>Komponenten ließen sich nicht mehr einfach als `.wasm` austauschen (ADR 0003, Konsequenzen).</li></ul> |

### Frage 2 — Ablage und Standard

| Variante                                                                                                  | Vorteile                                                                                                                                                                                                                                                     | Nachteile                                                                                                                                                  |
| :-------------------------------------------------------------------------------------------------------- | :----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | :--------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **A. Standardmäßig an, eigener Ordner** (`component_cache_dir`, Standard `cache/components`)              | <ul><li>Der Gewinn gilt ohne Konfiguration.</li><li>Der Ordner liegt neben den anderen Host-Daten; `cache/` steht schon in `.gitignore`, und der Watcher überspringt ihn.</li><li>Kein Teilen mit anderen Programmen, also kein fremder Schreiber.</li></ul> | <ul><li>Ein relativer Pfad hängt wie `plugin_dir` vom Arbeitsverzeichnis ab.</li></ul>                                                                     |
| **B. Standardmäßig an, Standardordner von wasmtime** (macOS `~/Library/Caches/BytecodeAlliance.wasmtime`) | <ul><li>Unabhängig vom Arbeitsverzeichnis.</li></ul>                                                                                                                                                                                                         | <ul><li>Geteilt mit der `wasmtime`-CLI und anderen Programmen; mehr mögliche Schreiber.</li><li>Für Betreiber schwerer zu finden und zu löschen.</li></ul> |
| **C. Standardmäßig aus, nur per Opt-in**                                                                  | <ul><li>Kein neuer Ordner ohne ausdrückliche Zustimmung.</li></ul>                                                                                                                                                                                           | <ul><li>Der Gewinn bleibt für die meisten Nutzer aus.</li></ul>                                                                                            |

### Frage 3 — Unsicherer oder nicht nutzbarer Ordner

| Variante                              | Vorteile                                                                                                                                                                              | Nachteile                                                                         |
| :------------------------------------ | :------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | :-------------------------------------------------------------------------------- |
| **A. Ohne Cache weiter, mit Warnung** | <ul><li>Der Host startet immer.</li><li>Das Ergebnis ist dasselbe wie mit Cache, nur langsamer; nichts wird vorgetäuscht.</li><li>Die Warnung nennt den Grund und den Pfad.</li></ul> | <ul><li>Eine übersehene Warnung kostet nur Zeit, keine Sicherheit.</li></ul>      |
| **B. Start verweigern**               | <ul><li>Der Betreiber muss den Fehler beheben.</li></ul>                                                                                                                              | <ul><li>Ein reiner Geschwindigkeitsgewinn würde den Betrieb blockieren.</li></ul> |

## Entscheidung

1. **Mechanismus (1A).** Der Host nutzt den eingebauten Cache von wasmtime über `Config::cache`. Er ruft kein `Component::deserialize`/`deserialize_file` auf, und der Host bleibt ohne `unsafe`. Es wird nichts beim Build vorkompiliert.
2. **Ablage (2A).** Der Cache ist standardmäßig an.
   - `component_cache` (Standard `true`) schaltet ihn ab.
   - `component_cache_dir` (Standard `cache/components`) legt den Ordner fest. Ein relativer Pfad gilt wie `plugin_dir` relativ zum Arbeitsverzeichnis.
   - Der Host legt einen fehlenden Ordner selbst an und gibt ihn nur dem eigenen Benutzer (Unix `0700`).
3. **Vertrauensgrenze.** Vor dem Einschalten prüft der Host den Ordner. Er nutzt ihn nur, wenn alle drei Bedingungen gelten:
   - Er gehört dem Benutzer, unter dem der Host läuft.
   - Weder Gruppe noch andere dürfen hineinschreiben.
   - Er ist kein symbolischer Link.
4. **Rückfall (3A).** Ist der Ordner unsicher oder nicht nutzbar, etwa weil er sich nicht anlegen lässt oder die Cache-Konfiguration scheitert, kompiliert der Host wie bisher ohne Cache. Er protokolliert eine Warnung mit Pfad und Grund und startet normal. Fehler beim Lesen oder Schreiben einzelner Einträge behandelt wasmtime selbst als Cache-Fehlschlag; dann wird ebenfalls kompiliert.
5. **Nachvollziehbarkeit.** Beim Start meldet der Host auf `debug`, ob der Cache aktiv ist und welchen Ordner er nutzt.

Die Umsetzung folgt als Feature `component-cache` mit PRD und Tech Spec.

## Konsequenzen

**Positiv**

- Ab dem zweiten Start entfällt das Kompilieren weitgehend. Das beschleunigt vor allem `process` und `stream`.
- Eine geänderte Komponente oder ein wasmtime-Update führt zu einem neuen Eintrag, nie zu veraltetem Code.
- Der Host-Code bleibt ohne `unsafe`; die Verfassung ändert sich nicht.

**Negativ**

- Der Cache-Ordner ist eine neue Vertrauensgrenze. Wer ihn beschreiben kann, kann Code im Host ausführen. Die Rechteprüfung verringert das Risiko, beseitigt es aber nicht: Ein Prozess desselben Benutzers kann weiterhin hineinschreiben.
- Es gibt einen zusätzlichen Hintergrund-Thread von wasmtime für das Aufräumen.
- Der Ordner wächst bis zu den Standardgrenzen von wasmtime. Ältere Einträge werden von wasmtime gelöscht.
- Die Rechteprüfung ist auf Unix-Systemen definiert. Auf anderen Plattformen gilt der Ordner als nicht prüfbar, und der Host kompiliert ohne Cache (Rückfall 3A).

**Verfassung**

- Keine Regel ändert sich. Die Entscheidung hält _Strikte Regeln_ 1 ein und berücksichtigt _Security_ 1–3 durch die Rechteprüfung.
- Die Varianten 1B und 1C hätten eine Ausnahme von _Strikte Regeln_ 1 gebraucht.
