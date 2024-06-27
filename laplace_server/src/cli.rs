use std::path::PathBuf;

#[derive(clap::Parser)]
pub struct Opts {
    #[clap(short, long, default_value = "Laplace.toml")]
    pub config: PathBuf,
}
