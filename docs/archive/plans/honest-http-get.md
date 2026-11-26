---
feature: honest-http-get
status: archived
archive-after:
---

# Plan — http_get meldet Fehler statt erfundener Antworten (`honest-http-get`)

Reihenfolge der Umsetzung. Jede Task nennt das Akzeptanzkriterium, auf das sie einzahlt.

## Tasks

- [x] AC-1 `http_get` meldet einen nicht startbaren `curl` und einen Exit-Code ungleich 0 als Fehler mit Ursache.
- [x] AC-2 `curl --fail` für HTTP-Fehlerstatus; leere Antwort und Antwort ohne UTF-8 sind Fehler.
- [x] AC-4 Erfundenen Antworttext aus `http_get` entfernt.
- [x] AC-5 `reversegeo`: Ersatzort entfernt; Warnung mit Ursache, keine Namen, kein Cache.
- [x] AC-3 Tests für `weather-enrich` und `reversegeo` gegen das echte `http_get`; READMEs und CHANGELOG.
