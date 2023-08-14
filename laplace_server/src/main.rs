use clap::Parser;
use laplace_server::settings::Settings;

mod cli;

#[actix_web::main]
async fn main() {
    let opts: cli::Opts = cli::Opts::parse();
    let settings = Settings::new(&opts.config).expect("Settings should be configured");

    laplace_server::init_logger(&settings.log).expect("Logger should be configured");
    laplace_server::run(settings).await.expect("Laplace running error")
}
