---
status: accepted
date: 2026-11-27
supersedes: []
references:
  - docs/adr/0002-plugins-without-engine-no-output.md
  - docs/adr/0003-wasm-component-execution.md
---

# ADR 0007 — HTTP-Client in Rust statt curl für die http-Capability

## Kontext

Die Capability `http` (`CapabilityManager::http_get`, `crates/host/src/capabilities.rs`) startet für jede Anfrage das Systemprogramm `curl`:

```text
curl -sS -L --fail --max-time 2 -H "User-Agent: flagme-host/<version>" <url>
```

Danach liest sie die ganze Standardausgabe ein. Seit #52 meldet sie jeden Fehlschlag als Fehler und erfindet keine Antwort (ADR 0002). Aufrufer sind `weather-enrich` (Open-Meteo) und `reversegeo` (Nominatim-Rückfall). Issue #66 zeigt die Schwächen:

| Bereich              | Befund                                                                                                                                                                                          |
| :------------------- | :---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Laufzeitabhängigkeit | `curl` muss auf dem `PATH` liegen; sonst scheitert jede Anfrage.                                                                                                                                |
| Kosten               | Ein Prozessstart je Anfrage, kein Wiederverwenden von Verbindungen.                                                                                                                             |
| Antwortgröße         | Unbegrenzt. Die Speichergrenze des Gasts (`max_memory_bytes`) erfasst keine Puffer im Host (ADR 0003, Entscheidung 5). Eine große Antwort belegt Host-Speicher, bis der Timeout von 2 s greift. |
| Weiterleitungen      | `-L` folgt bis zu 50 Weiterleitungen (Standard von `curl`), ohne eigene Grenze.                                                                                                                 |
| Fehlertexte          | Die Ursache stammt aus dem stderr von `curl` (`curl exit <n>`) und hängt von dessen Version ab.                                                                                                 |
| Proxy, TLS           | `curl` wertet die Proxy-Variablen aus und nutzt den Zertifikatsspeicher des Systems. Festgelegt ist beides nirgends.                                                                            |
| Verfassung           | _Strikte Regeln_ 1 verlangt sicheres, idiomatisches Rust im Produktionscode. Ein externes C-Programm im Pfad einer Capability liegt außerhalb davon. arc42 führt das als Risiko 2.              |

**Prototyp am 2026-11-27** (`ureq` 3.5.2 mit rustls, Release-Build, macOS):

| Anfrage                                          | Ergebnis                                                                                                                     |
| :----------------------------------------------- | :--------------------------------------------------------------------------------------------------------------------------- |
| Open-Meteo-Archiv (HTTPS)                        | ok, 839 Byte, ≈ 100–140 ms                                                                                                   |
| Nominatim `/status`                              | ok, ≈ 180 ms                                                                                                                 |
| 10 MB Antwort, Grenze 4 MiB                      | Fehler „response body is larger than request limit: 4194304“                                                                 |
| HTTP 404                                         | Fehler „http status: 404“                                                                                                    |
| 8 Weiterleitungen, Grenze 5                      | Fehler „too many redirects“                                                                                                  |
| nicht erreichbare Adresse                        | Fehler „timeout: global“ nach 2,00 s                                                                                         |
| selbstsigniertes Zertifikat (System-Zertifikate) | Fehler „invalid peer certificate … not trusted“                                                                              |
| abgelaufenes Zertifikat (System-Zertifikate)     | Fehler erst durch den Timeout nach 2 s. Vermutlich prüft macOS den Widerrufsstatus online; das Tech Spec muss es nachmessen. |

Eine eigenständige Binärdatei mit `ureq`, rustls und System-Zertifikaten ist ≈ 2,8 MB groß. Das Host-Binary ist heute ≈ 29 MB groß.

Diese ADR klärt vier Fragen:

1. Welche Bibliothek ersetzt `curl`?
2. Welchen Zertifikaten vertraut der Client?
3. Welche Grenzen gelten, und wo werden sie festgelegt?
4. Wie geht der Client mit Proxys um?

## Varianten

### Frage 1 — Bibliothek

| Variante                                            | Vorteile                                                                                                                                                                                                                                                                                                          | Nachteile                                                                                                                                                 |
| :-------------------------------------------------- | :---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | :-------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **A. `ureq` 3**                                     | <ul><li>Blockierend; passt zu den synchronen Capability-Aufrufen des wasmtime-Linkers.</li><li>Rust mit rustls; keine Laufzeit wie tokio.</li><li>Größenlimit für den Body, Grenze für Weiterleitungen, Gesamt-Timeout, Proxy aus der Umgebung und Statusfehler sind eingebaut und im Prototyp geprüft.</li></ul> | <ul><li>Weitere Abhängigkeiten (rustls, `ring`); `ring` enthält Assembler- und C-Anteile für die Kryptografie.</li></ul>                                  |
| **B. `reqwest::blocking`**                          | <ul><li>Weit verbreitet, viele Funktionen.</li></ul>                                                                                                                                                                                                                                                              | <ul><li>Die blockierende Variante startet eine eigene tokio-Laufzeit in einem Hintergrund-Thread.</li><li>Mehr Abhängigkeiten, größeres Binary.</li></ul> |
| **C. `hyper` direkt** (über axum schon im Lockfile) | <ul><li>Keine neue HTTP-Bibliothek.</li></ul>                                                                                                                                                                                                                                                                     | <ul><li>TLS, Weiterleitungen, Timeout und Größenlimit wären eigener, sicherheitsrelevanter Code.</li></ul>                                                |

### Frage 2 — Zertifikate

| Variante                                               | Vorteile                                                                                                                                                                                    | Nachteile                                                                                                                   |
| :----------------------------------------------------- | :------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | :-------------------------------------------------------------------------------------------------------------------------- |
| **A. System-Zertifikate** (`rustls-platform-verifier`) | <ul><li>Verhält sich wie `curl` heute: macOS-Schlüsselbund, unter Linux der System-Store.</li><li>Eigene und Firmen-CAs funktionieren.</li><li>Widerrufsprüfung durch das System.</li></ul> | <ul><li>Unter Linux braucht es installierte `ca-certificates`.</li><li>Das Verhalten hängt vom System ab.</li></ul>         |
| **B. Mitgelieferte Mozilla-Liste** (`webpki-roots`)    | <ul><li>Reproduzierbar und unabhängig vom System.</li></ul>                                                                                                                                 | <ul><li>Eigene und Firmen-CAs werden nicht erkannt.</li><li>Neue Wurzeln kommen nur mit Updates der Abhängigkeit.</li></ul> |

### Frage 3 — Grenzen

| Variante                            | Vorteile                                                                                                                                                 | Nachteile                                                                                                      |
| :---------------------------------- | :------------------------------------------------------------------------------------------------------------------------------------------------------- | :------------------------------------------------------------------------------------------------------------- |
| **A. Feste Werte im Host**          | <ul><li>Einfach; wie der Timeout heute (`HTTP_TIMEOUT_SECS`).</li><li>Die heutigen Antworten sind klein (Open-Meteo ≈ 1 KB, Nominatim ≈ 2 KB).</li></ul> | <ul><li>Wer andere Werte braucht, muss den Host neu bauen.</li></ul>                                           |
| **B. Einstellbar in `config.yaml`** | <ul><li>Flexibel, auch je Plugin.</li></ul>                                                                                                              | <ul><li>Größerer Umfang, mehr Tests und mehr Dokumentation für einen Bedarf, den es noch nicht gibt.</li></ul> |

### Frage 4 — Proxy

| Variante                                                                                  | Vorteile                                                                                           | Nachteile                                                                |
| :---------------------------------------------------------------------------------------- | :------------------------------------------------------------------------------------------------- | :----------------------------------------------------------------------- |
| **A. Umgebungsvariablen beachten** (`HTTPS_PROXY`, `HTTP_PROXY`, `ALL_PROXY`, `NO_PROXY`) | <ul><li>Wie `curl` heute; bestehende Setups bleiben gleich.</li><li>Standard von `ureq`.</li></ul> | <ul><li>Das Verhalten hängt von der Umgebung des Prozesses ab.</li></ul> |
| **B. Ignorieren**                                                                         | <ul><li>Vorhersagbar.</li></ul>                                                                    | <ul><li>Bricht Umgebungen, die einen Proxy brauchen.</li></ul>           |

## Entscheidung

1. **`ureq` 3 (1A).**
   - Der Host nutzt `ureq` mit rustls und baut einen gemeinsamen `Agent` je `PluginRuntime`. Verbindungen dürfen wiederverwendet werden.
   - `curl` wird vollständig entfernt. Es gibt keinen Rückfall auf `curl` und keinen zweiten Pfad (wie ADR 0003).
2. **System-Zertifikate (2A).** TLS prüft mit `rustls-platform-verifier` gegen den Zertifikatsspeicher des Systems. Ein Zertifikat, dem das System nicht vertraut, lässt die Anfrage mit einem Fehler scheitern. Es gibt keine Option, die Prüfung abzuschalten.
3. **Feste Grenzen (3A).** Konstanten im Host:
   - Gesamt-Timeout 2 s, wie heute;
   - Antwort höchstens 1 MiB;
   - höchstens 5 Weiterleitungen, nur über `http`/`https`.

   Eine Überschreitung endet mit `Err(PluginError::Io)`, der die Grenze nennt. Der Body wird nie über die Grenze hinaus gelesen. Grenzen aus der Konfiguration sind eine spätere Erweiterung.

4. **Proxy aus der Umgebung (4A).** Der Client wertet `HTTPS_PROXY`, `HTTP_PROXY`, `ALL_PROXY` und `NO_PROXY` aus, wie `curl`. Der Deployment Guide nennt das.
5. **Fehlervertrag bleibt** (#52, ADR 0002). `http_get` liefert entweder den echten Antworttext oder `Err(PluginError::Io("http_get <url>: <Ursache>"))`.
   - Ursachen: Verbindungsfehler, Timeout, HTTP-Status ≥ 400 (mit Status), kein UTF-8, leere Antwort, Größen- oder Weiterleitungsgrenze, Zertifikatsfehler.
   - Der Host baut nie selbst einen Antworttext.
   - Der `User-Agent` bleibt `flagme-host/<version>`, wie die Nutzungsregeln von Nominatim es verlangen.

Die Umsetzung folgt als Feature mit PRD und Tech Spec. Das Tech Spec misst die Größe des Host-Binarys und das Verhalten bei abgelaufenen und widerrufenen Zertifikaten.

## Konsequenzen

**Positiv**

- Kein externes Programm mehr im Pfad der Capability; `curl` ist keine Laufzeitvoraussetzung mehr.
- Host-Speicher je Anfrage ist auf 1 MiB begrenzt. Das schließt die Lücke neben `max_memory_bytes`.
- Weiterleitungen sind begrenzt, und die Fehlertexte hängen nicht mehr von einer `curl`-Version ab.
- Kein Prozessstart je Anfrage; Verbindungen können wiederverwendet werden.

**Negativ**

- Neue Abhängigkeiten: `ureq`, rustls, `rustls-platform-verifier`, `ring`. Unter macOS kommt Security.framework hinzu. `ring` enthält Assembler- und C-Code für die Kryptografie. Das ist eine geprüfte Bibliothek, nicht Projektcode; _Strikte Regeln_ 1 betrifft den Code des Projekts.
- Das Host-Binary wird größer; das Tech Spec misst, um wie viel.
- Die Fehlertexte ändern sich (`curl exit <n>` entfällt). Tests und Dokumentation, die sie prüfen, werden angepasst.
- Unter Linux ohne `ca-certificates` scheitern alle HTTPS-Anfragen. Der Deployment Guide nennt das.
- Ein abgelaufenes Zertifikat kann unter macOS erst am Timeout scheitern. Ein Fehler ist es in jedem Fall.

**Verfassung**

- Keine Regel ändert sich. Die Entscheidung setzt _Strikte Regeln_ 1 für den Pfad der Capability um und stärkt _Security_ 3 durch die Größengrenze im Host.
- `sqlite3` als zweites externes Programm (arc42, Risiko 2) bleibt ein eigenes Thema.
