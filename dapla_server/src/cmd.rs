use std::path::PathBuf;

use clap::Clap;

#[derive(Clap)]
pub struct CmdOpts {
    #[clap(short, long, default_value = "settings.toml")]
    pub settings_path: PathBuf,
}
