---
feature: redacted-avif-thumbnails
status: accepted
archive-after:
touches:
  - plugins/thumbnail/**
  - plugins/privacy-redact/**
  - crates/models/**
  - crates/sdk/**
  - crates/host/src/**
  - crates/host/ui/**
  - crates/host/tests/**
  - wit/**
  - Cargo.toml
  - Cargo.lock
  - config*.yaml
  - NOTICE
  - README.md
  - CHANGELOG.md
  - docs/arc42.md
  - docs/pipeline_host_supervisor.md
  - docs/plugin-development-guide.md
---

# Tech Spec — Geschwärzte Thumbnails als AVIF (`redacted-avif-thumbnails`)

Setzt das PRD `docs/prd/redacted-avif-thumbnails.md` um. Die Verfassung hat Vorrang; diese Tech Spec darf sie präzisieren, nie lockern. Grundlage sind [ADR 0012](../adr/0012-redacted-avif-thumbnails.md) und [ADR 0011](../adr/0011-working-image-pure-rust-decoders.md).

## Architektur

```
metadata ─▶ image-convert ─▶ … ─▶ face-detection ─▶ privacy-redact ─▶ thumbnail
                                        │                 │                 │
                                  doc.faces     face_redaction +     display_source(&doc)
                                                redacted_image_path   None ⇒ keine Thumbnails
```

**Datenmodell** (`crates/models`)

```rust
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FaceRedaction { Applied, NoFaces, Disabled, Failed }

pub struct PrivacyInfo {
    // … bisherige Felder …
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub face_redaction: Option<FaceRedaction>,
}
```

`None` bedeutet „unbekannt“; das betrifft Dokumente älterer Läufe. Gespeichert wird das Feld über `documents_store.doc_json`; die Tabelle `privacy_redactions` und die Migration bleiben unverändert.

**WIT** (`wit/media-plugin.wit`, Paket bleibt `@0.3.0`, weil unveröffentlicht)

```wit
/// Result of the face redaction step of privacy-redact (ADR 0012).
enum face-redaction { applied, no-faces, disabled, failed }

record privacy-info {
    // … bisherige Felder …
    face-redaction: option<face-redaction>,
}
```

Host (`crates/host/src/component.rs`) und SDK (`crates/sdk/src/component.rs`) wandeln in beiden Richtungen um.

**SDK** (`crates/sdk/src/lib.rs`)

```rust
/// Image a plugin may use for images that are shown (ADR 0012); `None`: faces could not be redacted.
pub fn display_source(doc: &UnifiedDocument) -> Option<&str> {
    match doc.privacy.as_ref().and_then(|p| p.face_redaction) {
        Some(FaceRedaction::Applied) => doc.privacy.as_ref()?.redacted_image_path.as_deref(),
        Some(FaceRedaction::Failed) => None,
        _ => Some(pixel_source(doc)),
    }
}
```

**`privacy-redact`** (`plugins/privacy-redact/src/lib.rs`, `process`)

| Fall                                                                  | `face_redaction` | `redacted_image_path`            | Warnung                                                     |
| :-------------------------------------------------------------------- | :--------------- | :------------------------------- | :---------------------------------------------------------- |
| `redact_faces: false`                                                 | `disabled`       | `None`                           | —                                                           |
| `redact_faces` und `doc.faces = None`                                 | `failed`         | `None`                           | wie bisher „no face detection result …; faces NOT redacted“ |
| `redact_faces`, `doc.faces` leer                                      | `no_faces`       | `None`                           | —                                                           |
| `redact_faces`, Gesichter, Schreiben gelingt                          | `applied`        | `<output_dir>/<id>.redacted.jpg` | —                                                           |
| `redact_faces`, Gesichter, Lesen, Dekodieren oder Schreiben scheitert | `failed`         | `None`                           | wie bisher „faces in … NOT redacted: …“                     |

- `PrivacySettings::default().output_dir` wird `cache/thumbnails`.
- GPS und Seriennummern bleiben wie sie sind.
- `doc.privacy` wird immer gesetzt (bisher schon).

**`thumbnail`** (`plugins/thumbnail`)

- Abhängigkeit: `oxideav-avif = { version = "=0.0.18", default-features = false, features = ["registry"] }`, wie in `image-convert`.
- `ThumbnailSettings`:
  - **`format: ThumbnailFormat`** (`Avif` | `Jpeg`), Standard `Avif`. Gelesen mit `r.choice("format", &[], &[(&["avif"], Avif), (&["jpeg", "jpg"], Jpeg)], &[], base.format)`; ein anderer Wert warnt und nimmt den Standard.
  - `quality: Option<u32>` bleibt. Der Standard hängt vom Format ab: `DEFAULT_AVIF_QUALITY = 70`, `DEFAULT_JPEG_QUALITY = 75`.
- `process`:
  1. Bei `display_source(&doc) == None`: Warnung `thumbnail: '<file.path>': faces could not be redacted; no thumbnails (ADR 0012)`, `doc.thumbnails` leer, `Ok(doc)`. Es wird nichts gelesen und nichts geschrieben.
  2. Sonst Quelle lesen und `decode_oriented` wie bisher. Das geschwärzte Bild und das Arbeitsbild sind JPEG ohne EXIF; ein Original liest es wie bisher.
  3. Je Größe `render_thumbnail(&img, size, format, quality)`:
     - `Jpeg`: wie bisher;
     - `Avif`: `oxideav_avif::encode_rgb8(w, h, rgb, &EncodeOptions::default().with_quality(q).with_threads(1).with_grid(GridTiling::MaxTile(1024)))`, `EncodeSpeed::Fast` ist der Standard der Crate.
  4. Dateiname `<cache_key>_<size>.<avif|jpg>`.
- `cache_key` (aus `file.path` und Größe), Größen und Beschriftungen bleiben.

**Host**

- `config.rs`:
  - `AppConfig::default().pipeline`: `thumbnail` ans Ende, nach `privacy-redact`.
  - Neue Methode `pub fn validate_privacy_order(&self) -> Result<(), String>`. Die Regel gilt, wenn `privacy-redact` in `pipeline` steht und `enabled` ist und `settings.redact_faces` fehlt oder `parse_bool` wahr ergibt:
    - `face-detection` steht in `pipeline`, ist `enabled` und steht vor `privacy-redact`. Sonst kommt `pipeline: face-detection must be enabled and run before privacy-redact while redact_faces is on (ADR 0012)`;
    - `thumbnail` steht, falls aktiv, nach `privacy-redact`. Sonst kommt `pipeline: thumbnail (position <n>) must run after privacy-redact (position <m>) while redact_faces is on, otherwise thumbnails show unredacted faces (ADR 0012)`.
  - `load_from_str` ruft die Prüfung am Ende auf, für JSON und YAML, und gibt einen Fehler zurück. `load_from_file` erbt das.
- `server.rs`, `get_photo_thumbnail`:
  - `thumb` wie bisher (Name, sonst die erste);
  - `None` oder nicht lesbare Datei ergibt `StatusCode::NOT_FOUND`;
  - `FALLBACK_JPEG_1X1` und der Rückfall auf `file.path` entfallen;
  - `Content-Type` nach Endung: `.avif` → `image/avif`, `.jpg`/`.jpeg` → `image/jpeg`, sonst `application/octet-stream`;
  - `Cache-Control: private, max-age=86400`.
- `ui/index.html`:
  - Das große Bild im Detail-Dialog bekommt wie die Kacheln einen `onerror`-Platzhalter (📷);
  - die Zeile „Privacy“ zeigt `face_redaction`.

**Profile**

- In allen fünf Profilen steht `thumbnail` am Ende der Pipeline.
- `plugins.thumbnail.settings.format: 'avif'`.
- Grenzen nach PRD:
  - `config_2cores_4GB.yaml`: `resource_limits.timeout_ms: 60000`;
  - `config_cpu_8GB.yaml`: `timeout_ms: 30000`;
  - in `config_mac-studio_64GB.yaml` entfällt der eigene Block.
- Die Mac-Studio-Profile setzen `variants: '128,256,512,1024'`.
- `quality` in den Profilen wird für AVIF gesetzt:
  - `config_2cores_4GB.yaml` `60`;
  - `config_cpu_8GB.yaml` `70`;
  - Mac Studio `80`.

  Bisher waren es JPEG-Werte (80/90/95).

- `privacy-redact` setzt in keinem Profil `output_dir`.

## Schnittstellen

- **WIT:** `enum face-redaction`, Feld `privacy-info.face-redaction` (Paket `@0.3.0`).
- **SDK:** `flagme_sdk::display_source`.
- **Settings `thumbnail`:** neu `format` (`avif` | `jpeg`); `quality` mit Standard je Format.
- **Settings `privacy-redact`:** Standard `output_dir` ist `cache/thumbnails`.
- **HTTP:** `GET /api/photos/:id/thumbnail/:size` gibt `404` statt Original oder Ersatzbild zurück. Dazu `Content-Type` nach Datei und `Cache-Control: private, max-age=86400`.
- **Konfiguration:** Eine Pipeline, die ADR 0012 verletzt, wird beim Laden mit Fehler abgelehnt. `flagme-host` beendet sich dann mit Exit-Code ≠ 0 und der Meldung auf stderr.

## Datenmodell

- `FaceRedaction` und `PrivacyInfo.face_redaction` wie oben.
- `ThumbnailEntry` bleibt unverändert.
- Keine Datenbankmigration: `face_redaction` steht im JSON von `documents_store`.

## Umsetzung der Akzeptanzkriterien

Tests in `crates/host/tests/redacted_thumbnails_test.rs`. Hilfen: `common::avif_dimensions(bytes)` dekodiert mit `oxideav-avif` (neue Dev-Abhängigkeit des Hosts) und prüft die Marke `avif` im `ftyp`.

| AC    | Umsetzung                                       | Test                                                                                                                                                                       |
| :---- | :---------------------------------------------- | :------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| AC-1  | Reihenfolge, `display_source`, `privacy-redact` | `test_ac1_thumbnails_are_redacted` (Komponenten, `AppConfig::default()` mit `style: blackout`; Gesichtsmitten je Größe ≈ schwarz, im Thumbnail des Originals nicht)        |
| AC-2  | `output_dir` von `privacy-redact`               | `test_ac2_no_cache_redacted` (Arbeitsverzeichnis mit Unterordnern prüfen; `face_redaction == applied`)                                                                     |
| AC-3  | `failed`, fail closed                           | `test_ac3_failed_redaction_no_thumbnails` (ohne Gesichtsergebnis; mit nicht lesbarem Arbeitsbild; mit `LogCaps.deny_write`)                                                |
| AC-4  | `disabled`, `no_faces`, ohne `privacy`          | `test_ac4_disabled_and_no_faces_use_working_image`                                                                                                                         |
| AC-5  | Server                                          | `test_ac5_thumbnail_endpoint_404_and_headers` (Router mit `tower::ServiceExt::oneshot`, wie `server_test.rs`)                                                              |
| AC-6  | `validate_privacy_order`                        | `test_ac6_pipeline_order_is_enforced` (falsche Reihenfolgen als YAML und JSON, `redact_faces: false`, `privacy-redact` aus; `AppConfig::default()` und `common::PROFILES`) |
| AC-7  | AVIF-Kodierung                                  | `test_ac7_avif_thumbnails` (Marke, Maße, Endung; `format: jpeg`)                                                                                                           |
| AC-8  | Settings                                        | `test_ac8_format_and_quality_defaults` (ungültiges `format`, Standardqualität, AVIF 1024 px < JPEG 1024 px)                                                                |
| AC-9  | WIT/JSON                                        | `test_ac9_face_redaction_roundtrip` (Probe über `countryflag`, JSON, altes Dokument ohne Feld)                                                                             |
| AC-10 | Grenzen                                         | `test_ac10_large_image_avif_within_limits` (4000 × 3000 im Test erzeugt, Komponenten, `AppConfig::default()`)                                                              |
| AC-11 | Dokumentation                                   | `test_ac11_docs_and_notice`                                                                                                                                                |

**Anpassungen bestehender Tests** (die Kennungen bleiben):

- Pipeline-Reihenfolge: `ac_test.rs`, `component_runtime_test.rs`, `examples_test.rs` sowie `image_convert_test.rs` AC-11 (`metadata`, `image-convert` bleiben vorn).
- Bisher JPEG-Thumbnails: `derivatives_test.rs` (no-synthetic-derivatives AC-5/6), `settings_test.rs` (thumbnail-Settings) und `image_convert_test.rs` AC-9 (Seitenverhältnis). Sie setzen `format: jpeg`, wo sie JPEG-Eigenschaften prüfen, oder prüfen AVIF.
- `privacy_test.rs`, `face_yunet_test.rs` AC-5: Standard `output_dir` und `face_redaction`.
- `server_test.rs`: Der Rückfall auf das Original entfällt; ein Test, der ihn erwartet, prüft jetzt `404`.

## Sicherheit

- **Fail closed:** Ohne geschwärztes Bild entsteht bei `redact_faces` kein Anzeigebild. Die Host-Prüfung verhindert eine Reihenfolge, die das umgehen würde.
- **Server:**
  - Er gibt nur Dateien aus `doc.thumbnails` heraus, nie `file.path` (Original) oder `working_image` (ungeschwärzt);
  - `private` verhindert, dass geteilte Caches oder Proxies Vorschauen speichern;
  - ohne Thumbnail gibt es `404` statt eines erfundenen Bilds (_Strikte Regeln_ 2, ADR 0002).
- **_Security_ 1–3:** Es gibt keine neue Capability. `oxideav-avif` läuft auch in `thumbnail` nur im Gast, mit dessen Grenzen.
- **_Architektur_ 3:** `thumbnail` kennt `privacy-redact` nicht, nur `doc.privacy` über `display_source`.
- **Altbestand:** Dokumente früherer Läufe behalten ungeschwärzte JPEG-Thumbnails, bis sie neu verarbeitet werden. Das CHANGELOG weist darauf hin (PRD, Nicht-Ziele).

## Ergänzungen zur Umsetzung

- **Mac-Studio-Profile** (Entscheidung des Maintainers vom 2026-11-28, im PRD nachgetragen): Sie erzeugen höchstens 1024 px. Deshalb entfällt die eigene 6-GB-Grenze von `thumbnail` im 64-GB-Profil.
- **`logging_test.rs`** (wasm-geo-host AC-10):
  - Die Tests schalteten `face-detection` ab, um eine sichere Warnung von `privacy-redact` zu bekommen. Diese Konfiguration lehnt der Host jetzt ab (AC-6).
  - Jetzt entziehen sie `thumbnail` den Grant `filesystem` und prüfen die Warnung `thumbnail: cannot read …`. Das Kriterium (Plugin-Warnung auf stderr, stdout nur JSON) bleibt gleich.
- **`derivatives_test.rs`** (no-synthetic-derivatives AC-5) prüft JPEG-Dateien und setzt dafür `format: Jpeg`.
- **README:** Der Befehl zum Bauen der Komponenten nannte `image-convert` noch nicht (Lücke aus ADR 0011); er ist ergänzt. Dasselbe gilt für `docs/pipeline_host_supervisor.md`, deren Ablaufdiagramm jetzt die Reihenfolge mit `image-convert` vorn und `thumbnail` am Ende zeigt.
- **Wackelnder Test, unabhängig von diesem Feature:**
  - `component_runtime_test.rs` `test_ac9_settings_warning_once` schlug in zwei von etwa zehn parallelen Läufen fehl (0 statt 1 Warnung); einzeln lief er sechsmal grün.
  - Ursache ist wahrscheinlich der Subscriber `tracing::subscriber::with_default`, der nur für den eigenen Thread gilt, während andere Tests im selben Testprogramm parallel laufen.
  - Der Test ist nicht geändert; ein Issue wird vorgeschlagen.

## Messwerte

Gemessen am 2026-11-28 auf macOS (Apple Silicon), Release-Build der Komponenten, Wasm-SIMD.

| Messung | Wert |
| :-- | --: |
| `flagme_plugin_thumbnail.wasm` mit AVIF-Encoder | 3.441.756 Byte |
| `thumbnail` über den Host, 4000 × 3000, AVIF 128/512/1024 px (AC-10, Grenzen 256 MB, 10 s) | 2,86 s |
| 1024 px von `apollo11.jpg`: AVIF q70 / JPEG q75 (AC-8) | 75.151 B / 123.503 B |
| ganze Pipeline mit `AppConfig::default()`, `apollo11.jpg` (AC-1) | 3 Gesichter, `face_redaction = applied`, 3 AVIF-Thumbnails mit geschwärzten Gesichtsmitten |

## Offene Punkte

- Keine.
