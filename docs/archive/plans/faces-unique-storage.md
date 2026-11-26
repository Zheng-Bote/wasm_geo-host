---
feature: faces-unique-storage
status: archived
archive-after:
---

# Plan — Eindeutige Gesichter in SQLite (`faces-unique-storage`)

Reihenfolge der Umsetzung. Jede Task nennt das Akzeptanzkriterium, auf das sie einzahlt.

## Tasks

- [x] AC-1 `face_row_id` und eindeutiger Zeilenschlüssel in `save_document`.
- [x] AC-2 Alte Gesichtszeilen des Fotos vor dem Einfügen löschen.
- [x] AC-3 `persons.representative_face_id` reparieren; Tests, README, CHANGELOG.
