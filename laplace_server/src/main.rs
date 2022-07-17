use clap::Parser;
use laplace_server::settings::Settings;

use self::cmd::CmdOpts;

mod cmd;

#[actix_web::main]
async fn main() {
    let cmd_opts: CmdOpts = CmdOpts::parse();
    let settings = Settings::new(&cmd_opts.settings_path).expect("Settings should be configured");

    laplace_server::init_logger(&settings.log);
    laplace_server::run(settings).await.expect("Laplace running error")
}
