---
status: active
---

# Verfassung — wasm_geo-host

Diese Verfassung enthält die systemweiten, dauerhaften Regeln des Projekts. Feature-Inhalte (User Stories, Tasklisten, Planung) gehören in PRD, Tech Spec und Plan. Jede Änderung einer Regel verweist auf ein ADR unter `docs/adr`.

## Architektur/Layering

1. Die Plattform gliedert sich strikt in die Schichten Host (`crates/host`), SDK (`crates/sdk`), Datenmodelle (`crates/models`) und Plugins (`plugins/*`).
2. Plugins werden als WebAssembly-Komponenten (Wasm Component Model) ausgeführt. Alle Schnittstellen zwischen Host und Plugins sind deklarativ in WIT (`wit/`) definiert.
3. Plugins dürfen ausschließlich vom SDK (`crates/sdk`) bzw. den gemeinsamen Datenmodellen abhängen. Kein Plugin darf von einem anderen Plugin abhängen oder Implementierungsdetails anderer Plugins kennen.
4. Alle Daten werden über das einheitliche Dokumentenmodell (`UnifiedDocument`) in einer typsicheren Pipeline weitergereicht.

## Security

1. Prinzip der geringsten Rechte (Capability-based Security): Plugins besitzen keinerlei Ambient Authority.
2. Jeder Zugriff auf Systemressourcen (Dateisystem, Netzwerk/HTTP, SQLite, Key-Value-Store, Secrets) erfordert eine explizite Capability-Freigabe pro Plugin in der Host-Konfiguration.
3. Wasmtime-Gäste laufen isoliert mit beschränkten Ressourcen (Memory Limits, Execution Timeouts).

## Strikte Regeln

1. Der gesamte Produktionscode (Host, SDK, Models, Plugins) wird in sicherem, idiomatischem Rust implementiert.
2. Es werden keine vereinfachten Mocks, Stubs oder Pseudocode-Fragmente im Produktionspfad akzeptiert.
3. Jeder Test nennt sein Akzeptanzkriterium direkt darüber in der Form `// <slug> AC-<n>: <Beschreibung>`.
4. Vor jedem Merge muss `sdd gate pre-merge` ohne Fehler durchlaufen.
