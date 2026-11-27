use std::path::{Path, PathBuf};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").canonicalize().unwrap()
}

fn model() -> PathBuf {
    let p = root().join("models/clip-vit-b32-text-int8.onnx");
    assert!(p.is_file(), "CLIP text model missing: run `make models`");
    p
}

// ai-tagging-clip AC-7: clip-labels produces byte-identical output from the same inputs, rejects duplicate labels and writes the header (magic, version, count, dimension)
#[test]
fn test_ac7_deterministic() {
    let text = "# comment\na dog\n\na cat   # trailing comment\n";
    let tokenizer = root().join("models/clip-vit-b32-tokenizer.json");
    let a = clip_labels::build(text, std::fs::read(model()).unwrap(), &tokenizer).unwrap();
    let b = clip_labels::build(text, std::fs::read(model()).unwrap(), &tokenizer).unwrap();
    assert_eq!(a, b, "deterministic");
    assert_eq!(&a[0..4], clip_labels::MAGIC);
    assert_eq!(u16::from_le_bytes([a[4], a[5]]), clip_labels::FORMAT_VERSION);
    assert_eq!(u32::from_le_bytes([a[8], a[9], a[10], a[11]]), 2);
    assert_eq!(u32::from_le_bytes([a[12], a[13], a[14], a[15]]) as usize, clip_labels::DIM);
    assert_eq!(a.len(), 80 + 2 * 512 * 4 + (2 + 5) + (2 + 5));
    assert!(clip_labels::parse_labels("a dog\na dog\n").unwrap_err().contains("duplicate"));
    assert_eq!(clip_labels::parse_labels(text).unwrap(), vec!["a dog", "a cat"]);
}
