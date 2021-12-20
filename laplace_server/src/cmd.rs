use std::path::PathBuf;

#[derive(clap::Parser)]
pub struct CmdOpts {
    #[clap(short, long, default_value = "settings.toml")]
    pub settings_path: PathBuf,
}
