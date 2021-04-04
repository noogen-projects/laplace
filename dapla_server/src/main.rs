use std::io;

use clap::Clap;
use dapla_server::settings::Settings;

use self::cmd::CmdOpts;

mod cmd;

#[actix_web::main]
async fn main() -> io::Result<()> {
    let cmd_opts: CmdOpts = CmdOpts::parse();
    let settings = Settings::new(&cmd_opts.settings_path).expect("Settings should be configured");
    env_logger::init_from_env(env_logger::Env::new().default_filter_or(settings.log.level.to_string()));

    dapla_server::run(settings).await
}
