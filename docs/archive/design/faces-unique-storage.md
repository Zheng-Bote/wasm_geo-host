---
feature: faces-unique-storage
status: archived
archive-after:
touches:
  - crates/host/src/**
  - crates/host/tests/**
  - plugins/face-detection/README.md
  - CHANGELOG.md
---

# Tech Spec — Eindeutige Gesichter in SQLite (`faces-unique-storage`)

Setzt das PRD `docs/prd/faces-unique-storage.md` um. Die Verfassung hat Vorrang; diese Tech Spec darf sie präzisieren, nie lockern.

## Architektur

Nur `crates/host/src/storage.rs` (`Storage::save_document`) ändert sich. Die Tabelle `faces` wird nicht zurückgelesen; Dokumente kommen aus `documents_store` (JSON) mit den Original-IDs.

## Schnittstellen

Für `doc.faces = Some(..)` erzeugt `save_document` in dieser Reihenfolge:

```sql
DELETE FROM faces WHERE photo_id = '<doc.id>';
INSERT OR IGNORE INTO persons (id, name, representative_face_id) VALUES ('<person>', '<person>', '<doc.id>/<face.id>');  -- je Person
INSERT OR REPLACE INTO faces (id, photo_id, …) VALUES ('<doc.id>/<face.id>', '<doc.id>', …);                         -- je Gesicht
UPDATE persons SET representative_face_id = (SELECT id FROM faces WHERE person_id = persons.id ORDER BY rowid LIMIT 1)
 WHERE representative_face_id IS NULL
    OR representative_face_id NOT IN (SELECT id FROM faces WHERE person_id = persons.id);
```

- `pub fn face_row_id(photo_id: &str, face_id: &str) -> String` liefert `<photo_id>/<face_id>`.
- `doc.faces = None` (nicht bestimmt) lässt bestehende Zeilen unverändert; `Some` mit leerer Liste löscht die Zeilen des Fotos.

## Datenmodell

Unverändert (`faces.id TEXT PRIMARY KEY`); nur der Wert ist jetzt global eindeutig.

## Umsetzung der Akzeptanzkriterien

| AC | Umsetzung | Test (`crates/host/tests/faces_storage_test.rs`) |
|---|---|---|
| AC-1 | `face_row_id` | `test_ac1_two_photos_two_rows` |
| AC-2 | `DELETE … WHERE photo_id` | `test_ac2_reprocessing_does_not_duplicate` |
| AC-3 | `UPDATE persons …` | `test_ac3_representative_face_exists` |

## Sicherheit

Keine Auswirkungen; Werte laufen weiter durch `escape_sql`.

## Offene Punkte

- Keine.
