use clap::Parser;
use flexi_logger::{Duplicate, FileSpec, Logger};
use laplace_server::settings::{LoggerSettings, Settings};

use self::cmd::CmdOpts;

mod cmd;

#[actix_web::main]
async fn main() {
    let cmd_opts: CmdOpts = CmdOpts::parse();
    let settings = Settings::new(&cmd_opts.settings_path).expect("Settings should be configured");

    init_logger(&settings.log);

    laplace_server::run(settings).await.expect("Laplace running error")
}

fn init_logger(settings: &LoggerSettings) {
    let mut logger = Logger::try_with_env_or_str(&settings.spec).expect("Logger should be configured");
    if let Some(dir) = &settings.dir {
        logger = logger.log_to_file(
            FileSpec::default()
                .directory(dir)
                .basename("laplace")
                .suppress_timestamp()
                .suffix("log"),
        );
    }
    logger
        .duplicate_to_stdout(if settings.duplicate_to_stdout {
            Duplicate::All
        } else {
            Duplicate::None
        })
        .start()
        .expect("Logger should be started");
}
