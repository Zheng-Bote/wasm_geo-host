---
status: accepted
date: 2026-11-26
supersedes: []
references: []
---

# ADR 0003 — Plugins laufen als eingebettete Wasm-Komponenten

## Kontext

Die Verfassung verlangt zwei Dinge. Plugins laufen als WebAssembly-Komponenten, und alle Schnittstellen zwischen Host und Plugin sind in WIT definiert (_Architektur/Layering_ 2). Gäste laufen isoliert mit Speicher- und Zeitgrenzen (_Security_ 3). Issue #30 zeigt, dass der heutige Stand beides nicht erfüllt.

- **`flagme-host process`, `watch`, `serve`:** Alle 11 Plugins sind nativ in den Host gelinkt (`register_plugin(Arc::new(XyzPlugin::new()))`). Sie laufen als `Arc<dyn Plugin>` im Host-Prozess. Der Host hängt weder von `wasmtime` ab, noch nutzt er `PluginConfig.path`.
- **`flagme-host stream`:** Der Host startet die externe `wasmtime`-CLI mit den CLI-Binaries der Plugins und reicht JSON über stdin/stdout weiter. Nur Stufe 0 erhält ein Verzeichnis (`--dir`). Capabilities, `enabled` und `settings` aus `config.yaml` greifen hier nicht.
- **Bibliotheks-Builds:** Die `wasm32-wasip2`-Bibliotheken exportieren nichts. `wasm-tools component wit` zeigt `world root {}`.
- **Ressourcengrenzen:** `resource_limits.max_memory_bytes` und `timeout_ms` werden gelesen, aber nirgends durchgesetzt.
- **WIT-Vertrag:** `wit/media-plugin.wit` ist veraltet.
  - `document` fehlen `ai`, `weather`, `ocr`, `faces`, `privacy` und `location.local_*`. Stattdessen stehen dort `ai-tags` und `custom-json`.
  - `host-capabilities` fehlt `fs-write`.
  - `process` erhält keine `settings` (#26).

Die Plugin-Logik selbst ist dafür vorbereitet. Seit #25 läuft jeder Datei-, Netz-, KV- und Secret-Zugriff über `HostCapabilities`. Seit #26 kommen die Settings über `PluginContext`. Kein Plugin nutzt mehr `std::fs` oder externe Prozesse. Damit lässt sich jedes Plugin als Komponente bauen, die nur die Imports aus `host-capabilities` braucht.

Diese ADR klärt vier Fragen:

1. Wie führt der Host die Komponenten aus?
2. Wie wird das `UnifiedDocument` über die WIT-Grenze gereicht?
3. Was wird aus dem nativen Modus und aus `stream`?
4. Wie werden die Ressourcengrenzen durchgesetzt?

## Varianten

### Frage 1 — Ausführung

| Variante                                                                                                     | Vorteile                                                                                                                                                                                                                                                                                                                                                                                          | Nachteile                                                                                                                                                                                                                                                                                                                                                                     |
| :----------------------------------------------------------------------------------------------------------- | :------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | :---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **A. `wasmtime` als Bibliothek im Host** (Component Model, `wasmtime::component::bindgen!`, `wasmtime-wasi`) | <ul><li>Erfüllt _Architektur_ 2 und _Security_ 3 direkt.</li><li>Capabilities werden als typisierte Imports an den `CapabilityManager` je Plugin gebunden.</li><li>Speicher- und Zeitgrenzen gelten je Aufruf.</li><li>Jede Komponente wird einmal kompiliert (`Component`, `InstancePre`) und je Dokument in einem eigenen `Store` instanziiert.</li><li>Kein externes Programm nötig.</li></ul> | <ul><li>Größere Abhängigkeit: `wasmtime` mit Cranelift verlängert die Build-Zeit und vergrößert das Binary.</li><li>Eine API-Version von `wasmtime` muss gepflegt werden.</li><li>Komponenten müssen vor dem Start gebaut sein.</li></ul>                                                                                                                                     |
| **B. Subprozesse mit der `wasmtime`-CLI** (Ausbau von `stream`)                                              | <ul><li>Bestehender Code.</li><li>Isolation auf Prozessebene.</li><li>Kein `wasmtime` im Host-Binary.</li></ul>                                                                                                                                                                                                                                                                                   | <ul><li>Die CLI kennt `host-capabilities` nicht. Capabilities ließen sich nur grob über `--dir`/`-S` abbilden, also ohne Grant je Plugin, ohne KV, Secrets und Host-Logging.</li><li>Die Schnittstelle wäre JSON über stdio statt WIT, was _Architektur_ 2 verfehlt.</li><li>Ein Prozessstart je Stufe und Dokument.</li><li>`wasmtime` muss auf dem `PATH` liegen.</li></ul> |
| **C. Anderer Runtime** (z. B. `wasmer`, `jco`)                                                               | <ul><li>Keine nennenswerten Vorteile für dieses Projekt.</li></ul>                                                                                                                                                                                                                                                                                                                                | <ul><li>Das Component Model ist dort weniger ausgereift als in `wasmtime`.</li><li>Die Verfassung nennt ausdrücklich Wasmtime-Gäste.</li></ul>                                                                                                                                                                                                                                |

### Frage 2 — Dokument über die WIT-Grenze

| Variante                                                                                                                                                    | Vorteile                                                                                                                                                                                                                                                                      | Nachteile                                                                                                                                                                                                                                                                                                              |
| :---------------------------------------------------------------------------------------------------------------------------------------------------------- | :---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | :--------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **A. Typisierte WIT-Records**, die `UnifiedDocument` vollständig abbilden; nur `custom` (`HashMap<String, serde_json::Value>`) wird als JSON-Text übergeben | <ul><li>Der Vertrag steht deklarativ in WIT, wie _Architektur_ 2 es verlangt.</li><li>Typsichere Pipeline nach _Architektur_ 4.</li><li>Fehler zeigen sich beim Binden, nicht erst zur Laufzeit.</li><li>Komponenten in anderen Sprachen können den Vertrag nutzen.</li></ul> | <ul><li>Das Schema steht doppelt, in `crates/models` und in WIT; es droht wieder Drift wie heute.</li><li>Umwandlungscode auf beiden Seiten (Gast im SDK, Host).</li><li>`HashMap`-Felder (`raw_tags`) werden zu `list<tuple<string, string>>`.</li><li>Jede Modelländerung ändert die WIT-Version.</li></ul>          |
| **B. JSON-Text mit Schemaversion** (`process: func(doc-json: string, …)`)                                                                                   | <ul><li>Eine einzige Quelle des Schemas (serde in `crates/models`).</li><li>Kaum Umwandlungscode.</li><li>Neue Felder ändern WIT nicht.</li></ul>                                                                                                                             | <ul><li>WIT beschreibt nur noch einen String, der eigentliche Vertrag liegt außerhalb. Das widerspricht _Architektur_ 2 („Alle Schnittstellen … deklarativ in WIT“) und bräuchte eine Verfassungsänderung.</li><li>Fehler zeigen sich erst zur Laufzeit.</li><li>Serialisierung kostet bei jeder Stufe Zeit.</li></ul> |
| **C. Mischform**: typisierter Kern (Datei, Metadaten, Ort, Zeit), angereicherte Teile als JSON                                                              | <ul><li>Weniger Umwandlungscode als A.</li></ul>                                                                                                                                                                                                                              | <ul><li>Zwei Mechanismen statt einem.</li><li>Die Grenze zwischen Kern und angereicherten Teilen ist willkürlich.</li><li>Erfüllt _Architektur_ 2 nur teilweise.</li></ul>                                                                                                                                             |

Gegen die Drift bei Variante A gibt es zwei Maßnahmen. Ein Round-Trip-Test wandelt ein vollständig befülltes `UnifiedDocument` nach WIT und zurück und vergleicht verlustfrei. Ein Test je Feld sorgt dafür, dass ein neues Feld in `crates/models` ohne WIT-Entsprechung den Build bricht. Die Umwandlungen liegen im SDK (Gastseite) und im Host. Plugins bleiben damit frei von WIT-Typen, und _Architektur_ 3 bleibt gewahrt.

### Frage 3 — Nativer Modus und `stream`

| Variante                                                                                                    | Vorteile                                                                                                                                                                                                             | Nachteile                                                                                                                                                                           |
| :---------------------------------------------------------------------------------------------------------- | :------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | :---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **A. Native Plugins aus dem Host-Binary entfernen**                                                         | <ul><li>Nur ein Ausführungspfad, und der ist sandboxed.</li><li>Kein stilles Umgehen der Capabilities.</li><li>Host und Plugins sind entkoppelt; Plugins lassen sich tauschen, ohne den Host neu zu bauen.</li></ul> | <ul><li>Host-Integrationstests brauchen gebaute Komponenten oder testen die Plugin-Logik getrennt.</li><li>Debugging im Gast ist mühsamer.</li></ul>                                |
| **B. Nativ als ausdrücklicher Entwicklungsmodus** (Cargo-Feature `native-plugins`, nicht im Standard-Build) | <ul><li>Schnelles Debugging.</li><li>Bestehende Tests laufen weiter.</li></ul>                                                                                                                                       | <ul><li>Zwei Ausführungspfade mit unterschiedlicher Isolation, die auseinanderlaufen können.</li><li>Das Risiko, dass der unsichere Pfad versehentlich ausgeliefert wird.</li></ul> |
| **C. Nativ als Rückfall**, wenn eine `.wasm` fehlt                                                          | <ul><li>Der Host startet immer.</li></ul>                                                                                                                                                                            | <ul><li>Umgeht die Sandbox still. Das widerspricht _Security_ 1–3.</li><li>Abgelehnt.</li></ul>                                                                                     |

Für `stream` gibt es zwei Wege: Der Modus läuft künftig über denselben eingebetteten Runtime, mit einem eigenen `Store` je Stufe, oder er entfällt. Die CLI-Binaries der Plugins (`[[bin]]`, JSON über stdin/stdout) bleiben als eigenständige Werkzeuge erhalten. Der Host nutzt sie nicht mehr.

### Frage 4 — Ressourcengrenzen

| Mechanismus                                                                         | Vorteile                                                                                                   | Nachteile                                                                                                                         |
| :---------------------------------------------------------------------------------- | :--------------------------------------------------------------------------------------------------------- | :-------------------------------------------------------------------------------------------------------------------------------- |
| **Speicher: `StoreLimits`** (`ResourceLimiter`, `memory_size` = `max_memory_bytes`) | <ul><li>Genau und ohne Laufzeitkosten.</li><li>Wird beim Wachsen des Speichers geprüft.</li></ul>          | <ul><li>Begrenzt nur den Linear Memory des Gasts. Host-seitige Puffer, etwa für `fs-read`-Ergebnisse, zählen nicht mit.</li></ul> |
| **Zeit: Epoch Interruption** (Ticker-Thread, Deadline aus `timeout_ms`)             | <ul><li>Geringer Overhead.</li><li>Entspricht der Wanduhrzeit, also dem, was `timeout_ms` meint.</li></ul> | <ul><li>Die Granularität hängt vom Ticker-Takt ab, etwa 10 ms.</li></ul>                                                          |
| **Zeit: Fuel**                                                                      | <ul><li>Deterministisch.</li></ul>                                                                         | <ul><li>Spürbarer Overhead.</li><li>Fuel lässt sich nicht in Millisekunden umrechnen.</li></ul>                                   |

## Entscheidung

1. **Einbettung (1A).** Der Host bettet `wasmtime` mit Component Model und `wasmtime-wasi` als Bibliothek ein. Er lädt jede aktivierte Komponente von `PluginConfig.path`. Jede Komponente wird einmal kompiliert und je Dokument in einem frischen `Store` instanziiert. Fehlt eine Komponente oder lässt sie sich nicht laden, startet der Host mit einer klaren Fehlermeldung nicht. Einen nativen Rückfall gibt es nicht.
2. **Typisierter WIT-Vertrag (2A).** `wit/media-plugin.wit` bildet `UnifiedDocument` vollständig ab. Nur `custom` wird als JSON-Text übergeben. Das Paket bekommt eine Version (`wasmgeohost:media@0.2.0`).
   - Signatur: `process: func(doc: document, settings: list<tuple<string, string>>) -> result<document, string>`.
   - `host-capabilities` enthält zusätzlich `fs-write`.
   - Ein Round-Trip-Test sichert ab, dass `crates/models` und WIT übereinstimmen.
3. **SDK-Adapter.** Das SDK stellt einen dünnen Adapter mit `wit-bindgen` bereit, zum Beispiel ein Makro `export_plugin!(XyzPlugin)`.
   - Er wandelt WIT-Typen in die Typen von `crates/models` und zurück.
   - Er baut den `PluginContext` aus Settings und Capability-Imports.
   - Plugin-Code implementiert weiter nur `Plugin`. Es entsteht keine Abhängigkeit zwischen Plugins.
4. **Keine Ambient Authority.** Gäste erhalten keine vorab geöffneten Verzeichnisse, keine Sockets, keine Umgebungsvariablen und kein stdin. Erlaubt sind nur WASI-Uhren, WASI-Zufall und stderr. Jeder Datei-, Netz-, KV- und Secret-Zugriff läuft über `host-capabilities` und wird je Plugin vom `CapabilityManager` geprüft. `log-event` landet im Host-Logging (`tracing`).
5. **Ressourcengrenzen.** Speicher wird über `StoreLimits` mit `max_memory_bytes` begrenzt, die Laufzeit über Epoch Interruption mit `timeout_ms`. Überschreitet ein Gast eine Grenze, endet der Aufruf mit einem Fehler. Die Pipeline protokolliert ihn und macht mit dem nächsten Dokument weiter. Die Grenzen gelten zunächst global aus `resource_limits`; Grenzen je Plugin sind eine spätere Erweiterung.
6. **Kein nativer Modus (3A).** Die Plugin-Crates fallen aus den `[dependencies]` von `flagme-host`. Sie bleiben `rlib`, damit ihre eigenen Unit-Tests die Logik direkt prüfen können; Tests gehören nicht zum Produktionspfad. Host-Tests, die die Ausführung prüfen, laden gebaute Komponenten.
7. **`stream` läuft über den eingebetteten Runtime.** Der Modus nutzt denselben Lade- und Capability-Pfad wie `process`, mit einem eigenen `Store` je Stufe, und beachtet damit `enabled`, `settings` und Capabilities. Die externe `wasmtime`-CLI wird nicht mehr aufgerufen. Die CLI-Binaries der Plugins bleiben als eigenständige Werkzeuge erhalten.

Die Umsetzung folgt als eigenes Feature mit PRD, Tech Spec und Plan. Die Akzeptanzkriterien orientieren sich an #30 (AC-1 bis AC-6).

## Konsequenzen

**Positiv**

- _Architektur/Layering_ 2 und _Security_ 1–3 werden zur Laufzeit eingehalten, nicht nur in der Dokumentation.
- Capabilities, Settings und Ressourcengrenzen gelten in allen Modi gleich: `process`, `watch`, `serve` und `stream`.
- Plugins lassen sich als `.wasm` austauschen, ohne den Host neu zu bauen. Komponenten in anderen Sprachen sind möglich.
- Ein fehlerhafter oder bösartiger Gast kann weder den Host-Speicher erschöpfen noch die Pipeline blockieren.

**Negativ**

- `wasmtime` verlängert die Build-Zeit und vergrößert das Host-Binary deutlich.
- Vor dem Start müssen die Komponenten gebaut sein (`cargo build --target wasm32-wasip2`).
- Die Dokumentation, `dist/` und das Docker-Image müssen nachgezogen werden.
- Ein Schemawechsel in `crates/models` betrifft zwei Stellen (WIT und Umwandlung). Der Round-Trip-Test macht Drift sichtbar, verhindert den Aufwand aber nicht.
- Über WIT geht jedes Dokument zweimal pro Stufe durch eine Umwandlung. Bei großen Feldern wie Embeddings kostet das messbar Zeit, bleibt aber gegenüber Bilddekodierung und Netzzugriffen klein.
- Host-Tests brauchen einen Wasm-Build. CI und `cargo test` müssen den Build-Schritt vorschalten.

**Folgen für weitere Arbeit**

- Echte Inferenz-Engines (ADR 0002, Punkt 6) laufen damit im Gast. Ob sie WASI-NN oder eine reine Rust-Engine wie `tract` nutzen, entscheidet deren eigene ADR.
- Grenzen je Plugin und ein Cache für kompilierte Komponenten (`Component::serialize`) sind spätere Erweiterungen.

**Verfassung**

- Keine Regel ändert sich. Die Entscheidung setzt _Architektur/Layering_ 2 und 4 sowie _Security_ 1–3 um.
- Die abgelehnte Variante 2B (JSON-Text) hätte eine Verfassungsänderung gebraucht.
