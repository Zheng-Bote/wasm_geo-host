//! `watch` reports the initial scan, shows skipped duplicates and supports `--once` (issue #71).

mod common;

use std::io::{BufRead, BufReader};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

/// Test directory with `inbox/` (watched) and a config whose database lives outside the inbox.
/// The pipeline is `metadata`, `timeline`: fast, and ADR 0012 does not apply without privacy-redact.
fn setup(tag: &str) -> (PathBuf, PathBuf, PathBuf) {
    let dir = common::temp_dir(tag);
    let inbox = dir.join("inbox");
    std::fs::create_dir_all(&inbox).unwrap();
    let config = dir.join("config.yaml");
    std::fs::write(
        &config,
        format!(
            "database_path: \"{}\"\nplugin_dir: \"{}\"\ncomponent_cache: false\npipeline:\n  - metadata\n  - timeline\n",
            dir.join("media.db").display(),
            common::components_dir().display()
        ),
    )
    .unwrap();
    (dir, inbox, config)
}

fn host(dir: &Path, args: &[&str]) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_flagme-host"));
    cmd.args(args).current_dir(dir).env_remove("RUST_LOG");
    cmd
}

fn watch_once(dir: &Path, inbox: &Path, config: &Path, extra: &[&str]) -> Output {
    let mut args = vec!["watch", inbox.to_str().unwrap(), "--config", config.to_str().unwrap(), "--once"];
    args.extend_from_slice(extra);
    host(dir, &args).output().expect("flagme-host runs")
}

fn stdout_lines(out: &Output) -> Vec<String> {
    String::from_utf8_lossy(&out.stdout).lines().map(str::to_string).collect()
}

fn summaries(lines: &[String]) -> Vec<&String> {
    lines.iter().filter(|l| l.starts_with("[watcher] Initial scan of")).collect()
}

/// A decodable JPEG; `seed` gives it a size and therefore a SHA-256 of its own.
fn jpeg(seed: u32) -> Vec<u8> {
    common::plain_jpeg(16 + seed, 12 + seed)
}

/// Runs `watch` without `--once`; collects stdout lines until `until` matches (or the time runs out).
struct Daemon {
    child: Child,
    lines: mpsc::Receiver<String>,
    seen: Vec<String>,
}

impl Daemon {
    fn start(dir: &Path, inbox: &Path, config: &Path) -> Self {
        let mut child = host(dir, &["watch", inbox.to_str().unwrap(), "--config", config.to_str().unwrap(), "--workers", "2"])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("flagme-host starts");
        let stdout = child.stdout.take().unwrap();
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if tx.send(line).is_err() {
                    break;
                }
            }
        });
        Self { child, lines: rx, seen: Vec::new() }
    }

    fn wait_for(&mut self, pred: impl Fn(&str) -> bool) -> String {
        let deadline = Instant::now() + Duration::from_secs(120);
        while Instant::now() < deadline {
            if let Ok(line) = self.lines.recv_timeout(Duration::from_millis(200)) {
                self.seen.push(line.clone());
                if pred(&line) {
                    return line;
                }
            }
        }
        panic!("line not seen within 120 s; output so far:\n{}", self.seen.join("\n"));
    }
}

impl Drop for Daemon {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

// watch-summary AC-1: after the initial scan exactly one summary line reports found, processed, already ingested, ignored and failed counts matching the directory, ends with the note that watching continues and appears only after all files are processed
#[test]
fn test_ac1_initial_scan_summary() {
    let (dir, inbox, config) = setup("ws_ac1");
    std::fs::write(inbox.join("old.jpg"), jpeg(1)).unwrap();
    let first = watch_once(&dir, &inbox, &config, &[]);
    assert!(first.status.success(), "{}", String::from_utf8_lossy(&first.stderr));

    std::fs::write(inbox.join("new1.jpg"), jpeg(2)).unwrap();
    std::fs::write(inbox.join("new2.jpg"), jpeg(3)).unwrap();
    std::fs::write(inbox.join("._new1.jpg"), vec![0u8; 4096]).unwrap();
    std::fs::write(inbox.join(".hidden.jpg"), jpeg(4)).unwrap();
    std::fs::write(inbox.join("notes.txt"), b"not an image").unwrap();

    let mut daemon = Daemon::start(&dir, &inbox, &config);
    let summary = daemon.wait_for(|l| l.starts_with("[watcher] Initial scan of"));
    assert!(
        summary.ends_with(": 6 files: 2 processed, 1 already ingested (skipped), 3 ignored (hidden/unsupported), 0 failed; watching for new files (Ctrl+C to stop)"),
        "{}",
        summary
    );
    // Both new files were reported before the summary, the old one as skipped.
    let before: Vec<&String> = daemon.seen.iter().take_while(|l| !l.starts_with("[watcher] Initial scan of")).collect();
    for name in ["new1.jpg", "new2.jpg"] {
        assert!(before.iter().any(|l| l.starts_with(&format!("[watcher] Processed '{}'", name))), "{:?}", before);
    }
    assert!(before.iter().any(|l| l.starts_with("[watcher] Skipped 'old.jpg': already ingested as ")), "{:?}", before);
    // Exactly one summary: nothing more arrives for the initial scan.
    std::thread::sleep(Duration::from_millis(800));
    while let Ok(line) = daemon.lines.try_recv() {
        daemon.seen.push(line);
    }
    assert_eq!(summaries(&daemon.seen).len(), 1, "{:?}", daemon.seen);
    drop(daemon);
    let _ = std::fs::remove_dir_all(&dir);
}

// watch-summary AC-2: a file whose SHA-256 is already stored gives a visible skip line with the existing document id at the default log level; the file stays where it is
#[test]
fn test_ac2_duplicate_is_visible_and_stays() {
    let (dir, inbox, config) = setup("ws_ac2");
    let out = dir.join("organized");
    std::fs::write(inbox.join("photo.jpg"), jpeg(5)).unwrap();
    let first = watch_once(&dir, &inbox, &config, &["--target-dir", out.to_str().unwrap()]);
    assert!(first.status.success(), "{}", String::from_utf8_lossy(&first.stderr));
    let processed = stdout_lines(&first).into_iter().find(|l| l.starts_with("[watcher] Processed 'photo.jpg'")).expect("processed");
    let id = processed.split("id=").nth(1).unwrap().split(' ').next().unwrap().to_string();
    assert!(!inbox.join("photo.jpg").exists(), "the first file was organized");

    std::fs::write(inbox.join("copy.jpg"), jpeg(5)).unwrap();
    let second = watch_once(&dir, &inbox, &config, &["--target-dir", out.to_str().unwrap()]);
    assert!(second.status.success());
    let lines = stdout_lines(&second);
    assert!(
        lines.contains(&format!("[watcher] Skipped 'copy.jpg': already ingested as {} (not organized)", id)),
        "{:?}",
        lines
    );
    assert!(inbox.join("copy.jpg").exists(), "the duplicate stays in the source directory");
    assert!(summaries(&lines)[0].contains(": 1 files: 0 processed, 1 already ingested (skipped), 0 ignored"), "{:?}", lines);
    let _ = std::fs::remove_dir_all(&dir);
}

// watch-summary AC-3: AppleDouble, hidden, derived and unsupported files are counted as ignored and not processed; with RUST_LOG=info each one is named
#[test]
fn test_ac3_ignored_files_counted_and_listed_at_info() {
    let (dir, inbox, config) = setup("ws_ac3");
    std::fs::create_dir_all(inbox.join("cache")).unwrap();
    let ignored = ["cache/x.jpg", "a.redacted.jpg", "b.working.jpg", "._c.jpg", ".d.jpg", "notes.txt"];
    for (n, name) in ignored.iter().enumerate() {
        std::fs::write(inbox.join(name), jpeg(10 + n as u32)).unwrap();
    }
    let out = host(&dir, &["watch", inbox.to_str().unwrap(), "--config", config.to_str().unwrap(), "--once"])
        .env("RUST_LOG", "info")
        .output()
        .unwrap();
    assert!(out.status.success());
    let lines = stdout_lines(&out);
    assert!(!lines.iter().any(|l| l.starts_with("[watcher] Processed")), "{:?}", lines);
    assert!(
        summaries(&lines)[0].ends_with(": 6 files: 0 processed, 0 already ingested (skipped), 6 ignored (hidden/unsupported), 0 failed; done"),
        "{:?}",
        lines
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    for name in ignored {
        let file = Path::new(name).file_name().unwrap().to_string_lossy().to_string();
        assert!(
            stderr.lines().any(|l| l.contains("Ignoring file") && l.contains(&file)),
            "{} not listed:\n{}",
            name,
            stderr
        );
    }
    // Default level: ignored files are only counted, not listed.
    let quiet = watch_once(&dir, &inbox, &config, &[]);
    assert!(!String::from_utf8_lossy(&quiet.stderr).contains("Ignoring file"));
    let _ = std::fs::remove_dir_all(&dir);
}

// watch-summary AC-4: watch --once processes the existing files, prints the summary ending in done and exits 0; a failing file gives exit code 1, counts as failed and is named in a Failed line; an empty directory gives 0 files and exit code 0
#[test]
fn test_ac4_once_exit_codes() {
    // Success.
    let (dir, inbox, config) = setup("ws_ac4");
    std::fs::write(inbox.join("ok.jpg"), jpeg(20)).unwrap();
    let ok = watch_once(&dir, &inbox, &config, &[]);
    assert_eq!(ok.status.code(), Some(0), "{}", String::from_utf8_lossy(&ok.stderr));
    let lines = stdout_lines(&ok);
    assert!(lines[0].starts_with("Importing ") && lines[0].contains(" once (workers: 4)"), "{:?}", lines);
    assert!(summaries(&lines)[0].ends_with(": 1 files: 1 processed, 0 already ingested (skipped), 0 ignored (hidden/unsupported), 0 failed; done"), "{:?}", lines);

    // Failure: the target directory cannot be written, so organizing fails.
    std::fs::write(inbox.join("blocked.jpg"), jpeg(21)).unwrap();
    let target = dir.join("readonly");
    std::fs::create_dir_all(&target).unwrap();
    std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o555)).unwrap();
    let failed = watch_once(&dir, &inbox, &config, &["--target-dir", target.to_str().unwrap()]);
    std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert_eq!(failed.status.code(), Some(1), "{}", String::from_utf8_lossy(&failed.stderr));
    let lines = stdout_lines(&failed);
    assert!(lines.iter().any(|l| l.starts_with("[watcher] Failed 'blocked.jpg': ")), "{:?}", lines);
    assert!(summaries(&lines)[0].contains(", 1 failed; done"), "{:?}", lines);

    // Empty and missing directories.
    let (dir2, inbox2, config2) = setup("ws_ac4_empty");
    let empty = watch_once(&dir2, &inbox2, &config2, &[]);
    assert_eq!(empty.status.code(), Some(0));
    assert!(summaries(&stdout_lines(&empty))[0].contains(": 0 files: 0 processed"), "{:?}", stdout_lines(&empty));
    let missing = watch_once(&dir2, &dir2.join("does-not-exist"), &config2, &[]);
    assert_eq!(missing.status.code(), Some(0));
    assert!(summaries(&stdout_lines(&missing))[0].contains(": 0 files: "));
    assert!(String::from_utf8_lossy(&missing.stderr).contains("does not exist"));
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_dir_all(&dir2);
}

// watch-summary AC-5: without --once, files created after the initial scan are still processed
#[test]
fn test_ac5_new_files_after_scan_are_processed() {
    let (dir, inbox, config) = setup("ws_ac5");
    let mut daemon = Daemon::start(&dir, &inbox, &config);
    let summary = daemon.wait_for(|l| l.starts_with("[watcher] Initial scan of"));
    assert!(summary.contains(": 0 files: ") && summary.ends_with("watching for new files (Ctrl+C to stop)"), "{}", summary);
    std::fs::write(inbox.join("later.jpg"), jpeg(30)).unwrap();
    daemon.wait_for(|l| l.starts_with("[watcher] Processed 'later.jpg'"));
    drop(daemon);
    let _ = std::fs::remove_dir_all(&dir);
}

// watch-summary AC-6: the usage text, the README and the deployment guide describe --once, the summary, the exit codes and duplicates; the README links the archived image-convert spec correctly
#[test]
fn test_ac6_usage_and_docs() {
    let dir = common::temp_dir("ws_ac6");
    let usage = host(&dir, &["watch"]).output().unwrap();
    assert!(String::from_utf8_lossy(&usage.stderr).contains("[--once]"));

    let root = common::repo_root();
    let readme = std::fs::read_to_string(root.join("README.md")).unwrap();
    let start = readme.find("### 6. Directory Watcher").expect("README section 6");
    let section = &readme[start..start + readme[start..].find("\n### 7.").unwrap_or(readme.len() - start)];
    let section = section.to_lowercase();
    for needle in ["--once", "initial scan of", "already ingested", "exit code", "stdout"] {
        assert!(section.contains(needle), "README section 6 lacks {}", needle);
    }
    let guide = std::fs::read_to_string(root.join("docs/deployment-guide.md")).unwrap().to_lowercase();
    for needle in ["--once", "initial scan of", "already ingested", "exit code"] {
        assert!(guide.contains(needle), "deployment guide lacks {}", needle);
    }
    assert!(!readme.contains("](docs/design/image-convert.md)") && !readme.contains("`docs/design/image-convert.md`"));
    assert!(readme.contains("docs/archive/design/image-convert.md"));
    assert!(root.join("docs/archive/design/image-convert.md").is_file());
    let _ = std::fs::remove_dir_all(&dir);
}
