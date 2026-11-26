mod common;

use flagme_host::storage::face_row_id;
use flagme_host::Storage;
use flagme_sdk::{BoundingBox, DetectedFace, FacesInfo, FileInfo, UnifiedDocument};
use std::path::Path;
use std::process::Command;

fn face(id: &str, x: f32, person: Option<&str>) -> DetectedFace {
    DetectedFace {
        id: id.to_string(),
        bbox: BoundingBox { x, y: 0.2, width: 0.2, height: 0.3 },
        confidence: 0.9,
        landmarks: None,
        embedding: None,
        person_id: person.map(str::to_string),
    }
}

fn photo(id: &str, faces: Vec<DetectedFace>) -> UnifiedDocument {
    let mut doc = UnifiedDocument::new(
        id,
        FileInfo {
            path: format!("/media/{}.jpg", id),
            filename: format!("{}.jpg", id),
            size_bytes: 1,
            mime_type: "image/jpeg".to_string(),
            sha256: None,
        },
    );
    doc.faces = Some(FacesInfo { faces, detection_model: "test".to_string() });
    doc
}

fn storage(dir: &Path) -> (Storage, String) {
    let db = dir.join("media.db").to_string_lossy().to_string();
    let storage = Storage::new(&db);
    storage.init_schema().unwrap();
    (storage, db)
}

/// Runs a query with the sqlite3 CLI and returns the rows ('|'-separated columns).
fn query(db: &str, sql: &str) -> Vec<String> {
    let out = Command::new("sqlite3").arg(db).arg(sql).output().expect("sqlite3 runs");
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout).lines().map(str::to_string).collect()
}

// faces-unique-storage AC-1: two photos with one face of the same detector id give two rows with the right photo_id
#[test]
fn test_ac1_two_photos_two_rows() {
    let dir = common::temp_dir("faces_ac1");
    let (storage, db) = storage(&dir);
    storage.save_document(&photo("photo-a", vec![face("face-1", 0.1, None)])).unwrap();
    storage.save_document(&photo("photo-b", vec![face("face-1", 0.5, None)])).unwrap();

    assert_eq!(
        query(&db, "SELECT id, photo_id FROM faces ORDER BY photo_id;"),
        vec!["photo-a/face-1|photo-a".to_string(), "photo-b/face-1|photo-b".to_string()]
    );
    assert_eq!(face_row_id("photo-a", "face-1"), "photo-a/face-1");
    let _ = std::fs::remove_dir_all(&dir);
}

// faces-unique-storage AC-2: reprocessing a photo adds no rows and removes faces that are gone
#[test]
fn test_ac2_reprocessing_does_not_duplicate() {
    let dir = common::temp_dir("faces_ac2");
    let (storage, db) = storage(&dir);
    let two = photo("photo-a", vec![face("face-1", 0.1, None), face("face-2", 0.5, None)]);
    storage.save_document(&two).unwrap();
    storage.save_document(&two).unwrap();
    assert_eq!(query(&db, "SELECT count(*) FROM faces WHERE photo_id = 'photo-a';"), vec!["2"]);

    storage.save_document(&photo("photo-a", vec![face("face-1", 0.1, None)])).unwrap();
    assert_eq!(query(&db, "SELECT id FROM faces WHERE photo_id = 'photo-a';"), vec!["photo-a/face-1"]);

    // "Detected, no faces" clears the rows; "not determined" (None) leaves them alone.
    let mut none = photo("photo-a", vec![]);
    none.faces = None;
    storage.save_document(&none).unwrap();
    assert_eq!(query(&db, "SELECT count(*) FROM faces WHERE photo_id = 'photo-a';"), vec!["1"]);
    storage.save_document(&photo("photo-a", vec![])).unwrap();
    assert_eq!(query(&db, "SELECT count(*) FROM faces WHERE photo_id = 'photo-a';"), vec!["0"]);
    let _ = std::fs::remove_dir_all(&dir);
}

// faces-unique-storage AC-3: persons.representative_face_id references an existing face of that person, also after reprocessing
#[test]
fn test_ac3_representative_face_exists() {
    let dir = common::temp_dir("faces_ac3");
    let (storage, db) = storage(&dir);
    let check = "SELECT p.id, f.photo_id FROM persons p JOIN faces f \
                 ON f.id = p.representative_face_id AND f.person_id = p.id ORDER BY p.id;";

    storage.save_document(&photo("photo-a", vec![face("face-1", 0.1, Some("anna"))])).unwrap();
    storage.save_document(&photo("photo-b", vec![face("face-1", 0.5, Some("anna"))])).unwrap();
    assert_eq!(query(&db, check), vec!["anna|photo-a"], "first face stays representative");

    // Photo A is reprocessed without Anna: the representative moves to her face in photo B.
    storage.save_document(&photo("photo-a", vec![face("face-1", 0.1, None)])).unwrap();
    assert_eq!(query(&db, check), vec!["anna|photo-b"]);
    assert_eq!(
        query(&db, "SELECT count(*) FROM persons WHERE representative_face_id NOT IN (SELECT id FROM faces);"),
        vec!["0"]
    );
    let _ = std::fs::remove_dir_all(&dir);
}
