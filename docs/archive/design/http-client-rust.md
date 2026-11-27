---
feature: http-client-rust
status: archived
archive-after:
touches:
  - crates/host/src/**
  - crates/host/tests/**
  - crates/host/Cargo.toml
  - Cargo.lock
  - README.md
  - docs/deployment-guide.md
  - docs/arc42.md
  - CHANGELOG.md
release: 0.3.0
---

# Tech Spec — HTTP-Client in Rust statt curl (`http-client-rust`)

Setzt das PRD `docs/prd/http-client-rust.md` um. Die Verfassung hat Vorrang; diese Tech Spec darf sie präzisieren, nie lockern. Grundlage ist [ADR 0007](../../adr/0007-http-client-rust.md).

## Architektur

**`crates/host/Cargo.toml`**

- `ureq = { version = "3.5", default-features = false, features = ["rustls", "platform-verifier"] }`. Der eingebaute Kryptografie-Provider von ureq ist `ring`; `gzip` entfällt.
- `[dev-dependencies]`, nur für den TLS-Test (AC-6):
  - `rcgen = "0.14"` für das selbstsignierte Zertifikat;
  - `rustls = { version = "0.23", default-features = false, features = ["ring", "std"] }` als Server.

**`crates/host/src/capabilities.rs`**

- Konstanten:
  - `HTTP_TIMEOUT_SECS = 2`;
  - `HTTP_MAX_RESPONSE_BYTES: u64 = 1_048_576`;
  - `HTTP_MAX_REDIRECTS: u32 = 5`;
  - `HTTP_USER_AGENT = concat!("flagme-host/", env!("CARGO_PKG_VERSION"))`.
- `pub fn http_agent() -> ureq::Agent` baut den Client mit:
  - `timeout_global(2 s)`, `max_redirects(5)`, `max_redirects_will_error(true)`;
  - `http_status_as_error(true)`, `user_agent(HTTP_USER_AGENT)`;
  - `tls_config(RootCerts::PlatformVerifier)`.

  Den Proxy liest der Standard von ureq aus der Umgebung (`Proxy::try_from_env`, mit `NO_PROXY`).

- `CapabilityManager` bekommt das Feld `http: ureq::Agent`.
  - `CapabilityManager::new` behält seine Signatur und ruft `http_agent()` auf.
  - Neu ist `CapabilityManager::with_http_agent(self, agent) -> Self`. Ein `ureq::Agent` ist ein günstiger Klon mit gemeinsamem Verbindungspool.
- `http_get(url)`:
  1. Grant `http` und Schema prüfen (unverändert).
  2. `agent.get(url).call()`.
  3. Den Body mit `body_mut().with_config().limit(HTTP_MAX_RESPONSE_BYTES).read_to_vec()` lesen.
  4. UTF-8 prüfen, auf leere Antwort prüfen.

  Fehler werden zu `PluginError::Io(format!("http_get {}: {}", url, cause))`:

  | `ureq::Error`                     | Ursache                              |
  | :-------------------------------- | :----------------------------------- |
  | `StatusCode(s)`                   | `HTTP status <s>`                    |
  | `Timeout(_)`                      | `timed out after 2 s`                |
  | `TooManyRedirects`                | `more than 5 redirects`              |
  | `BodyExceedsLimit(_)`             | `response larger than 1048576 bytes` |
  | `Tls(_)`, `Io(_)` mit TLS-Ursache | `TLS: <Text von ureq/rustls>`        |
  | sonst                             | Text von `ureq::Error` (`Display`)   |

  Dazu kommen `response is not UTF-8` und `empty response` (wie bisher).

- `std::process::Command` entfällt aus `capabilities.rs`.

**`crates/host/src/runtime.rs`**

- `PluginRuntime` hält ein `http: ureq::Agent`, gebaut einmal in `new`.
- `execute_plugin` übergibt es mit `CapabilityManager::new(…).with_http_agent(self.http.clone())`.

**`crates/host/src/component.rs`**

- Der Capability-Manager für `component_identity` bleibt ohne Grant. Er bekommt einen eigenen Agent über `new`, der nie benutzt wird. Weil ein Agent keinen Thread und keine Verbindung startet, kostet das nichts.

## Schnittstellen

- WIT, Plugins und SDK bleiben unverändert (`http-get: func(url) -> result<string, plugin-error>`).
- Konfiguration: keine neuen Schlüssel.
- Umgebung: `HTTPS_PROXY`, `HTTP_PROXY`, `ALL_PROXY` und `NO_PROXY` gelten für den Host-Prozess, gelesen beim Start der Laufzeit.

## Datenmodell

Keine Änderung.

## Umsetzung der Akzeptanzkriterien

| AC   | Umsetzung                      | Test                                                                                                                                                                                         |
| ---- | ------------------------------ | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| AC-1 | kein `Command::new("curl")`    | `crates/host/tests/http_client_env_test.rs` `test_ac1_works_without_curl_on_path` (eigenes Test-Binary, leert `PATH`) plus Quelltextprüfung `http_client_test.rs` `test_ac1_no_curl_in_host` |
| AC-2 | Fehlerabbildung                | `http_client_test.rs` `test_ac2_error_contract`                                                                                                                                              |
| AC-3 | `limit(1 MiB)`                 | `test_ac3_response_size_limit` (genau 1 MiB, 1 MiB + 1, endloser Datenstrom)                                                                                                                 |
| AC-4 | `max_redirects(5)`             | `test_ac4_redirect_limit` (5 ok, 6 Fehler, `ftp://` Fehler)                                                                                                                                  |
| AC-5 | `user_agent`, `timeout_global` | `test_ac5_user_agent_and_timeout`                                                                                                                                                            |
| AC-6 | `RootCerts::PlatformVerifier`  | `test_ac6_self_signed_certificate_rejected` (lokaler rustls-Server mit rcgen-Zertifikat)                                                                                                     |
| AC-7 | Proxy aus der Umgebung         | `http_client_env_test.rs` `test_ac7_proxy_from_environment` (setzt `HTTP_PROXY`/`NO_PROXY` vor dem Bau des Agents)                                                                           |
| AC-8 | Plugins unverändert            | bestehendes `http_get_test.rs` `test_ac3_…` (honest-http-get), erwarteter Text ohne `curl`                                                                                                   |
| AC-9 | Doku                           | `http_client_test.rs` `test_ac9_docs_without_curl_runtime`                                                                                                                                   |

**Anpassungen bestehender Tests** (die Kennungen bleiben):

- `http_get_test.rs`: Die Erwartung `curl exit` wird durch die Ursache ersetzt (`HTTP status 404`, Verbindungsfehler).
- `http_get_no_curl_test.rs` (`honest-http-get` AC-1, „curl nicht startbar“): Den Fall gibt es nicht mehr. Er geht in `http_client_env_test.rs` auf: Ohne `PATH` gelingt die Anfrage, und ein nicht erreichbarer Server liefert weiter einen Fehler.

Die Tests nutzen eigene lokale Server (`TcpListener`), kein externes Netz.

## Sicherheit

- _Strikte Regeln_ 1: Weder Host noch Tests enthalten `unsafe`, und es gibt kein externes Programm im Pfad der Capability.
- _Security_ 3: Der Host-Speicher je Anfrage ist auf 1 MiB begrenzt, die Wartezeit auf 2 s, die Weiterleitungen auf 5.
- TLS: Das Zertifikat wird immer geprüft; es gibt keinen Schalter zum Abschalten.
- Proxy: Werte aus der Umgebung sind Betreiber-Konfiguration, wie bei `curl`. Plugins können sie nicht setzen; sie haben keine Umgebung (ADR 0003).

## Messwerte

Gemessen am 2026-11-27 auf macOS (Apple Silicon), Release-Build.

| Messung | Ergebnis |
| :-- | :-- |
| Host-Binary `flagme-host` | 29.487.856 → 31.655.168 Byte (+ 2,2 MB) |
| `flagme-host process Examples/sample.jpg` mit leerem `PATH` | `weather-enrich` holt echte Daten von Open-Meteo über HTTPS in 90 ms (Wetter: 1,0 °C, „Overcast“) |
| abgelaufenes Zertifikat (`expired.badssl.com`), Timeout 15 s im Prototyp | korrekt abgelehnt: „certificate is expired“, nach 3,7 s |
| dasselbe mit dem festen Timeout von 2 s | scheitert am Timeout („timed out after 2 s“). Die Anfrage gelingt nie; die Ursache heißt nur nicht „expired“. |
| selbstsigniertes Zertifikat, lokal (AC-6) | sofort abgelehnt, Meldung enthält „certificate“ |

Die Prüfung unter macOS dauert bei manchen Zertifikaten länger als 2 s, vermutlich wegen Online-Abfragen zum Widerrufsstatus. Bei gültigen Zertifikaten (Open-Meteo, Nominatim) war davon nichts zu sehen.

## Offene Punkte

- Keine.
