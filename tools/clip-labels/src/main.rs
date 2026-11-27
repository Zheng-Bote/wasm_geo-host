//! `clip-labels --labels <txt> --model <text onnx> --tokenizer <json> --output <bin>` (see `make labels`).

use std::path::PathBuf;
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let value = |flag: &str| args.iter().position(|a| a == flag).and_then(|i| args.get(i + 1)).map(PathBuf::from);
    let (Some(labels), Some(model), Some(tokenizer), Some(output)) =
        (value("--labels"), value("--model"), value("--tokenizer"), value("--output"))
    else {
        eprintln!("usage: clip-labels --labels <labels.txt> --model <text_model.onnx> --tokenizer <tokenizer.json> --output <labels.bin>");
        return ExitCode::from(2);
    };
    match clip_labels::write(&labels, &model, &tokenizer, &output) {
        Ok(n) => {
            eprintln!("clip-labels: {} labels -> {}", n, output.display());
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("clip-labels: {}", e);
            ExitCode::FAILURE
        }
    }
}
