use std::env;
use std::path::PathBuf;
use std::sync::Once;

pub use laplace_client::*;
pub use laplace_service::*;

pub mod laplace_client;
pub mod laplace_service;
pub mod port;

pub fn init_logger() {
    static INIT: Once = Once::new();

    INIT.call_once(|| {
        let log_env = env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "debug,reqwest=info");
        let is_log_force = env::var("RUST_LOG_FORCE")
            .map(|force| !(force.trim().is_empty() || force.trim() == "0" || force.trim().to_lowercase() == "false"))
            .unwrap_or_default();

        env_logger::Builder::from_env(log_env).is_test(!is_log_force).init();
    });
}

pub fn target_build_dir() -> PathBuf {
    let mut dir = env::current_exe().expect("Cannot get current exe path");
    dir.pop();
    if dir.ends_with("deps") {
        dir.pop();
    }
    dir
}
