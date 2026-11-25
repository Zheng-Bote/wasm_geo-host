use std::io::Write;
use std::process::{Command, Stdio};

/// Host streaming pipeline supervisor executing WASM plugins as piped child processes.
pub struct StreamSupervisor {
    plugin_binaries: Vec<String>,
}

impl StreamSupervisor {
    pub fn new(plugin_binaries: Vec<String>) -> Self {
        Self { plugin_binaries }
    }

    pub fn default_pipeline() -> Self {
        Self::new(vec![
            "target/wasm32-wasip2/release/flagme-plugin-metadata.wasm".to_string(),
            "target/wasm32-wasip2/release/flagme-plugin-reversegeo.wasm".to_string(),
            "target/wasm32-wasip2/release/flagme-plugin-countryflag.wasm".to_string(),
            "target/wasm32-wasip2/release/flagme-plugin-thumbnail.wasm".to_string(),
            "target/wasm32-wasip2/release/flagme-plugin-ai-tagging.wasm".to_string(),
            "target/wasm32-wasip2/release/flagme-plugin-duplicate.wasm".to_string(),
            "target/wasm32-wasip2/release/flagme-plugin-timeline.wasm".to_string(),
            "target/wasm32-wasip2/release/flagme-plugin-weather-enrich.wasm".to_string(),
            "target/wasm32-wasip2/release/flagme-plugin-ocr-text.wasm".to_string(),
            "target/wasm32-wasip2/release/flagme-plugin-face-detection.wasm".to_string(),
            "target/wasm32-wasip2/release/flagme-plugin-privacy-redact.wasm".to_string(),
        ])
    }

    /// Execute the pipeline by streaming stdin/stdout between wasmtime instances.
    pub fn execute(&self, initial_image_path: &str) -> anyhow::Result<String> {
        let mut current_json = String::new();

        for (idx, wasm_path) in self.plugin_binaries.iter().enumerate() {
            let mut cmd = Command::new("wasmtime");
            cmd.arg("run");
            cmd.arg("-S").arg("cli-exit-with-code");

            if idx == 0 {
                // First stage: grant directory access and pass file argument
                cmd.arg("--dir").arg("Examples::/media");
                cmd.arg(wasm_path);
                let media_path = if initial_image_path.starts_with("Examples/") {
                    format!("/media/{}", &initial_image_path["Examples/".len()..])
                } else {
                    initial_image_path.to_string()
                };
                cmd.arg(&media_path);
                cmd.stdout(Stdio::piped());
                cmd.stderr(Stdio::inherit());

                let output = cmd.output()?;
                if !output.status.success() {
                    return Err(anyhow::anyhow!(
                        "Stage 0 ({}) failed with code {:?}",
                        wasm_path,
                        output.status.code()
                    ));
                }
                current_json = String::from_utf8(output.stdout)?;
            } else {
                // Subsequent stages: pipe JSON via stdin
                cmd.arg(wasm_path);
                cmd.stdin(Stdio::piped());
                cmd.stdout(Stdio::piped());
                cmd.stderr(Stdio::inherit());

                let mut child = cmd.spawn()?;
                if let Some(mut stdin) = child.stdin.take() {
                    stdin.write_all(current_json.as_bytes())?;
                }

                let output = child.wait_with_output()?;
                if !output.status.success() {
                    return Err(anyhow::anyhow!(
                        "Stage {} ({}) failed with code {:?}",
                        idx,
                        wasm_path,
                        output.status.code()
                    ));
                }
                current_json = String::from_utf8(output.stdout)?;
            }
        }

        Ok(current_json)
    }
}
