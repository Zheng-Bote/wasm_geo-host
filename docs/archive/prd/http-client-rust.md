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

# PRD — HTTP-Client in Rust statt curl (`http-client-rust`)

## Problem

Die Capability `http` startet für jede Anfrage das Systemprogramm `curl` (Issue #66). Das hat vier Folgen:

- `curl` muss installiert sein.
- Jede Anfrage kostet einen Prozessstart.
- Antworten werden unbegrenzt in den Host-Speicher gelesen; `max_memory_bytes` erfasst nur den Speicher des Gasts.
- Weiterleitungen sind nur durch den Standard von `curl` begrenzt (50).

Die Fehlertexte hängen von der `curl`-Version ab. Ein externes C-Programm im Pfad einer Capability liegt außerhalb von _Strikte Regeln_ 1.

## Ziel

`http_get` arbeitet im Host-Prozess mit `ureq` 3 und prüft TLS gegen die System-Zertifikate ([ADR 0007](../../adr/0007-http-client-rust.md)).

- Es gelten feste Grenzen: 2 s gesamt, 1 MiB Antwort, 5 Weiterleitungen.
- Proxy-Variablen werden wie bei `curl` beachtet.
- Der Fehlervertrag aus #52 bleibt unverändert: echte Antwort oder Fehler mit Ursache, nie ein erfundener Text.
- `curl` ist keine Laufzeitvoraussetzung mehr.

## Nutzer

- **Betreiber:** Sie brauchen `curl` nicht mehr und bekommen begrenzten Host-Speicher je Anfrage.
- **Plugin-Entwickler** (`weather-enrich`, `reversegeo`, künftige): Sie bekommen dieselbe Schnittstelle mit verlässlichen Fehlertexten.

## Umfang

- **Abhängigkeit:** `ureq` 3 in `flagme-host`, Features `rustls` und `platform-verifier`, ohne `gzip`.
- **`CapabilityManager`:**
  - nutzt einen `ureq::Agent`, den `PluginRuntime` einmal baut und an alle Capability-Manager weitergibt;
  - erzeugt `CapabilityManager::new` ohne übergebenen Agent einen eigenen, mit denselben Einstellungen.
- **Feste Grenzen:**
  - `HTTP_TIMEOUT_SECS = 2` (gesamt, wie bisher);
  - `HTTP_MAX_RESPONSE_BYTES = 1_048_576`;
  - `HTTP_MAX_REDIRECTS = 5`.
- **TLS:** System-Zertifikate über `rustls-platform-verifier`, ohne Option zum Abschalten der Prüfung.
- **Proxy:** `HTTPS_PROXY`, `HTTP_PROXY`, `ALL_PROXY` und `NO_PROXY` aus der Umgebung des Host-Prozesses.
- **Fehlervertrag:** `Err(PluginError::Io("http_get <url>: <Ursache>"))` für:
  - Verbindungsfehler, Timeout und HTTP-Status ≥ 400 (mit Status);
  - eine leere Antwort und eine Antwort, die kein UTF-8 ist;
  - Überschreiten der Größen- oder Weiterleitungsgrenze (mit Grenze);
  - Zertifikatsfehler.
- **Unverändert:** Schema-Prüfung (`http://`/`https://`), Grant `http` und `User-Agent: flagme-host/<version>`.
- **Tests anpassen:** Bestehende Tests, die `curl exit` oder `cannot run curl` erwarten, prüfen künftig die Ursache ohne `curl`. Ihre Kriterien-Kennungen bleiben.
- **Dokumentation:**
  - Deployment Guide: keine `curl`-Voraussetzung mehr; unter Linux `ca-certificates`; Proxy-Variablen;
  - arc42: Risiko 2, technischer Kontext, Verteilung;
  - README (Voraussetzungen), CHANGELOG.

## Nicht-Ziele

- Keine einstellbaren Grenzen in `config.yaml` (ADR 0007, Punkt 3).
- Kein Ersatz für die `sqlite3`-CLI in `Storage`.
- Keine anderen HTTP-Methoden als GET, keine Kompression, keine Cookies.
- Keine Positiv- oder Sperrliste von Hosts je Plugin.
- `curl` in `make geonames` und in Installationsanleitungen bleibt; es geht nur um den Laufzeitpfad der Capability.

## Akzeptanzkriterien

- **AC-1** Der Host-Code startet `curl` nicht mehr (`Command::new("curl")` kommt nicht vor). `http_get` gelingt auch, wenn `PATH` leer ist.
- **AC-2** Der Fehlervertrag aus `honest-http-get` gilt weiter, geprüft gegen einen lokalen Server: 200 liefert den echten Text; 404 und 500 liefern einen Fehler mit Status; eine leere Antwort, eine Antwort ohne UTF-8 und ein nicht erreichbarer Server liefern je einen Fehler mit Ursache. Jede Meldung beginnt mit `http_get <url>:` und enthält kein `curl`.
- **AC-3** Eine Antwort über 1 MiB scheitert mit einem Fehler, der die Grenze nennt. Das gilt auch, wenn der Server endlos sendet: Der Fehler kommt dann vor dem Timeout, weil höchstens bis zur Grenze gelesen wird. Eine Antwort von genau 1 MiB gelingt.
- **AC-4** 5 Weiterleitungen gelingen. 6 Weiterleitungen und eine Weiterleitung auf ein anderes Schema als `http`/`https` scheitern mit einem Fehler.
- **AC-5** Jede Anfrage trägt `User-Agent: flagme-host/<version>`. Ein Server, der nicht antwortet, führt nach höchstens 2 s (Toleranz bis 3 s) zu einem Timeout-Fehler.
- **AC-6** Ein HTTPS-Server mit selbstsigniertem Zertifikat lässt die Anfrage mit einem Zertifikatsfehler scheitern. Der Test läuft lokal, ohne Netz.
- **AC-7** Ist `HTTP_PROXY` gesetzt, läuft eine `http://`-Anfrage über diesen Proxy. Ein Host in `NO_PROXY` wird direkt erreicht.
- **AC-8** `weather-enrich` und `reversegeo` verhalten sich bei Fehlern wie bisher: Felder leer, Warnung mit Ursache, kein Cache-Eintrag (`honest-http-get` AC-3).
- **AC-9** Der Deployment Guide, arc42 und das CHANGELOG nennen `curl` nicht mehr als Laufzeitvoraussetzung. Sie beschreiben System-Zertifikate (`ca-certificates` unter Linux), die Proxy-Variablen und die festen Grenzen.
