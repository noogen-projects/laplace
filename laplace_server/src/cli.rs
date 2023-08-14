use std::path::PathBuf;

#[derive(clap::Parser)]
pub struct Opts {
    #[clap(short, long, default_value = "config.toml")]
    pub config: PathBuf,
}
