pub mod capabilities;
pub mod component;
pub mod config;
pub mod pipeline;
pub mod runtime;
pub mod storage;
pub mod watcher;
pub mod server;

pub use capabilities::CapabilityManager;
pub use config::AppConfig;
pub use pipeline::Pipeline;
pub use runtime::PluginRuntime;
pub use storage::Storage;
pub use watcher::{DirectoryWatcher, IngestOutcome, MediaOrganizer, WatchReport, WatcherConfig, process_single_file};
pub use server::{build_router, run_server, ServerState};
