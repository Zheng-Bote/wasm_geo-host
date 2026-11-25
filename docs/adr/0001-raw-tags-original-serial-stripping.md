---
status: accepted
date: 2026-11-25
supersedes: []
references: []
---

# ADR 0001 — raw_tags bleibt verlustfreies Original; Seriennummern-Entfernung auf Anforderung zulässig

## Kontext

`MetadataInfo.raw_tags` (`crates/models`) wird vom Plugin `metadata` befüllt und enthält jedes EXIF-, IPTC- und XMP-Tag unverändert, einschließlich der aggregierten JSON-Objekte unter den Schlüsseln `Exif`, `IPTC` und `XMP`. Nachgelagerte Plugins und der Host (FTS5-Spalte `raw_tags`) lesen diese Daten.

Das Plugin `privacy-redact` verändert personenbezogene Daten im `UnifiedDocument`:

- GPS-Fuzzing rundet `doc.location.latitude/longitude`.
- `strip_serial_numbers` entfernt Schlüssel mit `serialnumber` / `serial_number` aus `raw_tags`.

Bisher war nicht festgelegt, ob `raw_tags` als Originaldatensatz gilt oder von Datenschutz-Plugins bereinigt werden darf. Die Plugin-Dokumentation beschrieb die unveränderten GPS-Tags in `raw_tags` zunächst als Schwäche.

## Entscheidung

1. `raw_tags` ist der **verlustfreie Originaldatensatz** der Metadaten. Plugins verändern `raw_tags` grundsätzlich nicht.
2. **Ausnahme:** Das Entfernen von Hardware-Seriennummern aus `raw_tags` ist zulässig, wenn es ausdrücklich angefordert wird (`privacy-redact`, Setting `strip_serial_numbers` bzw. Alias `strip_serials`).
3. GPS-Fuzzing wirkt ausschließlich auf die abgeleiteten Felder in `doc.location`. GPS-Tags in `raw_tags` (z. B. `Exif.GPSLatitude`, `Exif.GPSLongitude`, aggregiertes `Exif`-JSON) bleiben unverändert.

## Konsequenzen

- Positiv: Der Originaldatensatz bleibt für Auswertung, Re-Processing und Audit erhalten; Datenschutz-Maßnahmen sind auf abgeleitete Felder begrenzt und nachvollziehbar (`doc.privacy.applied_rules`).
- Positiv: Seriennummern, die ein Gerät eindeutig identifizieren, können auf Anforderung auch im Originaldatensatz entfernt werden.
- Negativ: Exakte GPS-Koordinaten bleiben in `raw_tags` und damit im SQLite-Feld `metadata.raw_tags`, im `documents_store` und in der FTS5-Spalte `raw_tags` gespeichert. Ein Export oder eine Freigabe muss `raw_tags` daher gesondert behandeln.
- Offen: `strip_serial_numbers` filtert heute nur **Schlüssel**; im aggregierten JSON-String `raw_tags["Exif"]` bleiben `CameraSerialNumber` / `LensSerialNumber` enthalten. Ob die Entfernung auch diese Einträge umfassen muss, ist bei der Umsetzung zu klären.
- Keine Verfassungsregel ändert sich.
