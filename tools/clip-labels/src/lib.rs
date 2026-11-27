//! Builds `plugins/ai-tagging/labels/labels.bin` (ADR 0010): the CLIP text embedding of
//! "a photo of {label}" for every label of `labels.txt`.
//!
//! Format version 1, little endian: `"FCLB"`, u16 version, u16 reserved, u32 count, u32 dim,
//! 32-byte SHA-256 of `labels.txt`, 32-byte SHA-256 of the text model, `count * dim` f32
//! (L2-normalised), then every label as u16 length + UTF-8. The reader lives in
//! `plugins/ai-tagging/src/clip.rs`.

use rten::{Model, Value};
use rten_tensor::prelude::*;
use rten_tensor::{NdTensor, Tensor};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::path::Path;

pub const MAGIC: &[u8; 4] = b"FCLB";
pub const FORMAT_VERSION: u16 = 1;
pub const DIM: usize = 512;
pub const PROMPT_PREFIX: &str = "a photo of ";

/// Labels of a `labels.txt`: one per line, `#` starts a comment, blank lines are skipped.
/// Duplicates are an error.
pub fn parse_labels(text: &str) -> Result<Vec<String>, String> {
    let mut seen = HashSet::new();
    let mut labels = Vec::new();
    for (i, line) in text.lines().enumerate() {
        let label = line.split('#').next().unwrap_or("").trim();
        if label.is_empty() {
            continue;
        }
        if label.len() > u16::MAX as usize {
            return Err(format!("line {}: label too long", i + 1));
        }
        if !seen.insert(label.to_string()) {
            return Err(format!("line {}: duplicate label '{}'", i + 1, label));
        }
        labels.push(label.to_string());
    }
    if labels.is_empty() {
        return Err("no labels".to_string());
    }
    Ok(labels)
}

fn normalize(v: Vec<f32>) -> Result<Vec<f32>, String> {
    let n = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if !n.is_finite() || n == 0.0 {
        return Err("text embedding is zero or not finite".to_string());
    }
    Ok(v.into_iter().map(|x| x / n).collect())
}

/// Computes `labels.bin` from the label text, the text model and the tokenizer file.
pub fn build(labels_text: &str, model_bytes: Vec<u8>, tokenizer: &Path) -> Result<Vec<u8>, String> {
    let labels = parse_labels(labels_text)?;
    let model_sha: [u8; 32] = Sha256::digest(&model_bytes).into();
    let labels_sha: [u8; 32] = Sha256::digest(labels_text.as_bytes()).into();
    let tok = tokenizers::Tokenizer::from_file(tokenizer).map_err(|e| format!("tokenizer: {}", e))?;
    let model = Model::load(model_bytes).map_err(|e| format!("text model: {}", e))?;
    let ids_node = model.node_id("input_ids").map_err(|e| format!("text model input: {}", e))?;
    let out_node = model.node_id("text_embeds").map_err(|e| format!("text model output: {}", e))?;

    let mut out = Vec::new();
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&(labels.len() as u32).to_le_bytes());
    out.extend_from_slice(&(DIM as u32).to_le_bytes());
    out.extend_from_slice(&labels_sha);
    out.extend_from_slice(&model_sha);
    for label in &labels {
        let enc = tok.encode(format!("{}{}", PROMPT_PREFIX, label), true).map_err(|e| format!("'{}': {}", label, e))?;
        let ids: Vec<i32> = enc.get_ids().iter().map(|&i| i as i32).collect();
        let n = ids.len();
        let input = NdTensor::<i32, 2>::from_data([1, n], ids);
        let values = model
            .run(vec![(ids_node, Value::from(input).into())], &[out_node], None)
            .map_err(|e| format!("'{}': {}", label, e))?;
        let t: Tensor<f32> = values
            .into_iter()
            .next()
            .ok_or("no output")?
            .try_into()
            .map_err(|e| format!("'{}': unexpected output {:?}", label, e))?;
        let v = normalize(t.to_vec())?;
        if v.len() != DIM {
            return Err(format!("'{}': embedding has dimension {}, expected {}", label, v.len(), DIM));
        }
        for x in v {
            out.extend_from_slice(&x.to_le_bytes());
        }
    }
    for label in &labels {
        out.extend_from_slice(&(label.len() as u16).to_le_bytes());
        out.extend_from_slice(label.as_bytes());
    }
    Ok(out)
}

/// Builds and writes `labels.bin` atomically (`<output>.tmp`, then rename).
pub fn write(labels: &Path, model: &Path, tokenizer: &Path, output: &Path) -> Result<usize, String> {
    let text = std::fs::read_to_string(labels).map_err(|e| format!("{}: {}", labels.display(), e))?;
    let model_bytes = std::fs::read(model).map_err(|e| format!("{}: {} (run `make models`)", model.display(), e))?;
    let bytes = build(&text, model_bytes, tokenizer)?;
    let tmp = output.with_extension("bin.tmp");
    std::fs::write(&tmp, &bytes).map_err(|e| format!("{}: {}", tmp.display(), e))?;
    std::fs::rename(&tmp, output).map_err(|e| format!("{}: {}", output.display(), e))?;
    Ok(parse_labels(&text)?.len())
}
