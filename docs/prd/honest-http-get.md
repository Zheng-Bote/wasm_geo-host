---
feature: honest-http-get
status: draft
archive-after:
touches:
  - crates/host/src/**
  - crates/host/tests/**
  - plugins/reversegeo/**
  - plugins/weather-enrich/README.md
  - CHANGELOG.md
---

# PRD — http_get meldet Fehler statt erfundener Antworten (`honest-http-get`)

## Problem

`CapabilityManager::http_get` meldete eine gescheiterte Anfrage nie als Fehler (Issue #52). Betroffen waren diese Fälle: `curl` fehlt, Zeitüberschreitung, HTTP-Fehlerstatus, leere Antwort. In jedem lieferte die Funktion `Ok` mit einem erfundenen Text (`{"status": "offline", "url": …}`).

Dazu kommen zwei weitere Lücken:
- `curl` lief ohne `--fail`, also galten Antworten mit 4xx/5xx als Erfolg.
- Konnte `reversegeo` Koordinaten weder offline noch online auflösen, erfand es einen Ort (`"Unknown Country"`, `"XX"`, `"Global"`, Sprache `en`) und legte ihn im KV-Cache ab.

Beides widerspricht [ADR 0002](../adr/0002-plugins-without-engine-no-output.md) und _Strikte Regeln_ 2 der Verfassung.

## Ziel

Eine HTTP-Anfrage liefert entweder die echte Antwort des Servers oder einen Fehler, der die Ursache nennt. Plugins speichern bei einem Fehler nichts Erfundenes und nichts im Cache. Sie lassen die Felder leer und warnen.

## Nutzer

- **Betreiber:** Sie sehen in den Warnungen, warum Wetter oder Ortsnamen fehlen.
- **Plugin-Entwickler:** Sie können eine echte Antwort von einem Fehler unterscheiden.

## Umfang

- `CapabilityManager::http_get`:
  - `curl` mit `--fail`;
  - `Err(PluginError::Io)` mit Ursache für: `curl` nicht startbar, Exit-Code ungleich 0 (Zeitüberschreitung, Verbindung, HTTP-Status), kein UTF-8, leere Antwort.
- `reversegeo`:
  - kein Ersatzort mehr;
  - bei fehlendem Ergebnis bleiben die Namensfelder leer (die Koordinaten bleiben erhalten);
  - Warnung mit Ursache, kein Cache-Eintrag.
- `weather-enrich`: Das bestehende Verhalten (Warnung, kein Cache) wird gegen das echte `http_get` abgesichert.
- READMEs von `reversegeo` und `weather-enrich`, CHANGELOG.

## Nicht-Ziele

- Kein eigener HTTP-Client statt `curl`.
- Keine konfigurierbare Zeitgrenze (bleibt 2 s).
- Keine Wiederholungsversuche.
- `countryflag` kennt weiterhin den Code `XX`, wenn ihn eine Quelle ausdrücklich liefert. `reversegeo` erzeugt ihn nicht mehr.

## Akzeptanzkriterien

- **AC-1** Ist der Server nicht erreichbar oder lässt sich `curl` nicht starten, liefert `http_get` einen Fehler und keinen Antworttext.
- **AC-2** Eine Antwort mit HTTP 404 oder 500 lässt `http_get` mit einem Fehler scheitern, der den Status nennt. Eine leere Antwort ist ebenfalls ein Fehler. Eine Antwort mit 200 liefert den echten Text.
- **AC-3** Scheitert `http_get`, lassen `weather-enrich` und `reversegeo` ihre Felder leer, warnen mit der Ursache und schreiben nichts in den KV-Cache.
- **AC-4** Kein Codepfad von `http_get` baut selbst einen Antworttext. `reversegeo` enthält keinen Ersatzort mehr (`Unknown Country`, `XX`).
- **AC-5** Findet `reversegeo` keinen Ort, weil die Koordinaten nicht in den Offline-Daten liegen und `online_lookup` aus ist oder die Antwort unlesbar ist, bleiben die Namensfelder leer. Die Koordinaten bleiben erhalten, das Plugin warnt, und der Cache bleibt leer.
