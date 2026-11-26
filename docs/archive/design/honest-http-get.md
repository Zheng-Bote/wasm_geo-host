---
feature: honest-http-get
status: archived
archive-after:
touches:
  - crates/host/src/**
  - crates/host/tests/**
  - plugins/reversegeo/**
  - plugins/weather-enrich/README.md
  - CHANGELOG.md
---

# Tech Spec — http_get meldet Fehler statt erfundener Antworten (`honest-http-get`)

Setzt das PRD `docs/prd/honest-http-get.md` um. Die Verfassung hat Vorrang; diese Tech Spec darf sie präzisieren, nie lockern.

## Architektur

**`crates/host/src/capabilities.rs`, `CapabilityManager::http_get`**
- Nach der Prüfung des Grants `http` und des Schemas startet die Funktion `curl -sS -L --fail --max-time 2 -H "User-Agent: flagme-host/0.1.0" <url>`. Die Zeitgrenze steht in der neuen Konstante `HTTP_TIMEOUT_SECS`.
- Ergebnis:
  - `curl` nicht startbar → `Err(Io("http_get <url>: cannot run curl: …"))`.
  - Exit-Code ungleich 0 → `Err(Io("http_get <url>: <stderr von curl> (curl exit <n>)"))`. Mit `--fail` enthält stderr den HTTP-Status, z. B. `The requested URL returned error: 404`; bei einer Zeitüberschreitung `Operation timed out …`.
  - Antwort kein UTF-8 → `Err(Io("… response is not UTF-8"))`.
  - Antwort leer → `Err(Io("… empty response"))`.
  - Sonst → `Ok(body)`, der empfangene Text unverändert.

**`plugins/reversegeo/src/lib.rs`**
- Liefert der Offline-Lookup nichts, gibt es drei Fälle:
  - `online_lookup` aus → Warnung `… not in the offline data and online_lookup is off`.
  - `http_get` scheitert → Warnung mit dem Fehler.
  - Antwort nicht lesbar → Warnung `… unreadable geocoder response`.
- In allen drei Fällen geht das Dokument unverändert zurück: Die Koordinaten bleiben, die Namensfelder bleiben `None`, und es gibt keinen `kv_set`.
- Der bisherige Ersatzort (`Unknown Country`, `XX`, `Global`, `en`) entfällt.

**`weather-enrich`:** Code unverändert. Das Plugin warnt schon jetzt bei `Err` und bei unlesbarer Antwort und kehrt vor dem Cache zurück. Neu ist, dass ein Test das gegen das echte `http_get` absichert.

## Schnittstellen

- **Capability `http-get`:** Die WIT-Signatur bleibt unverändert. Fehler kommen als `plugin-error::io` mit Ursache.
- **Warnungen:**
  - `reversegeo: no location names for '<path>' (<lat>, <lon>): <cause>` (neu);
  - `weather-enrich: no measured weather for '<path>': <cause>` (bestehend, jetzt mit der echten Ursache).

## Datenmodell

Keine Änderung. `LocationInfo` kann jetzt Koordinaten ohne Namen enthalten. Das war schon vorher möglich, wenn `reversegeo` deaktiviert war. Die Persistenz speichert dann `NULL`.

## Umsetzung der Akzeptanzkriterien

| AC | Umsetzung | Test |
|---|---|---|
| AC-1 | Fehler bei nicht startbarem `curl` und bei Exit-Code ungleich 0 | `http_get_test::test_ac1_unreachable_server_is_an_error`, `http_get_no_curl_test::test_ac1_missing_curl_is_an_error` |
| AC-2 | `--fail`; Prüfung auf leere Antwort | `http_get_test::test_ac2_http_status_and_empty_body` (lokaler HTTP-Server) |
| AC-3 | Fehlerpfade in `reversegeo` und `weather-enrich` ohne Cache | `http_get_test::test_ac3_plugins_on_http_failure` (echter `CapabilityManager`) |
| AC-4 | Einziger `Ok`-Pfad ist `Ok(body)`; Ersatzort entfernt | `http_get_test::test_ac4_no_fabricated_responses_in_source` |
| AC-5 | Warnung und Rückgabe ohne Namen und Cache | `http_get_test::test_ac5_reversegeo_unresolved_coordinates`; `settings_test::reversegeo_online_lookup` angepasst |

Der Test für fehlendes `curl` leert `PATH` für den ganzen Prozess. Er liegt deshalb in einem eigenen Test-Binary.

## Sicherheit

- Keine Änderung an Grants oder Sandbox.
- Fehlertexte enthalten die URL und die Meldung von `curl`, aber keine Secrets; die URLs der Plugins enthalten keine.

## Offene Punkte

- Ein HTTP-Client in reinem Rust statt `curl`, eine konfigurierbare Zeitgrenze und Wiederholungsversuche sind spätere Erweiterungen.
- In bestehenden Datenbanken können noch Einträge mit `Unknown Country` / `XX` aus früheren Läufen stehen. Sie werden bei erneuter Verarbeitung ersetzt.
