---
feature: embedded-web-ui
status: draft
archive-after:
---

# Plan — embedded-web-ui (`embedded-web-ui`)

Reihenfolge der Umsetzung. Jede Task nennt das Akzeptanzkriterium, auf das sie einzahlt.

## Tasks

- [x] AC-1 Axum, rust-embed und tower-http Abhängigkeiten in `crates/host/Cargo.toml` aufnehmen.
- [x] AC-1 `crates/host/src/server.rs` implementieren: Axum Router, Server Startup & Lifecycle Management (`flagme-host serve`).
- [x] AC-2 REST-Endpunkte implementieren (`/api/photos`, `/api/photos/:id`, `/api/photos/:id/thumbnail/:size`, `/api/search`).
- [x] AC-3 GeoJSON-Endpunkt `/api/map/markers` implementieren mit `FeatureCollection` und Point-Geometrien.
- [x] AC-4 Web-Dashboard Assets in `crates/host/ui/index.html` anlegen und via `rust-embed` als Static Handler einbinden.
- [x] AC-1 CLI-Subcommand `flagme-host serve` in `crates/host/src/main.rs` integrieren.
- [x] AC-1, AC-2, AC-3, AC-4 Integrationstests in `crates/host/tests/server_test.rs` mit Kriterien-Referenzen (`// embedded-web-ui AC-<n>: ...`) erstellen und verifizieren.
