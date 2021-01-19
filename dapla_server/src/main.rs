use std::{io, path::PathBuf};

use actix_files::{Files, NamedFile};
use actix_web::{middleware, web, App, HttpServer};

use self::{
    daps::{Dap, DapsService},
    settings::Settings,
};

mod daps;
mod error;
mod settings;

#[actix_web::main]
async fn main() -> io::Result<()> {
    let settings = Settings::new().expect("Settings should be configured");
    env_logger::init_from_env(env_logger::Env::new().default_filter_or(settings.log.level.to_string()));

    let daps_service = DapsService::new(&settings.daps.path)?;

    HttpServer::new(move || {
        let static_dir = PathBuf::new().join(Dap::STATIC_DIR_NAME);

        let mut app = App::new()
            .data(daps_service.clone())
            .wrap(middleware::DefaultHeaders::new().header("X-Version", "0.2"))
            .wrap(middleware::NormalizePath::default())
            .wrap(middleware::Compress::default())
            .wrap(middleware::Logger::default())
            .service(Files::new(&format!("/{}", Dap::STATIC_DIR_NAME), &static_dir).index_file(Dap::INDEX_FILE_NAME))
            .route(
                "/",
                web::get().to(move || {
                    let index_file = static_dir.join(Dap::INDEX_FILE_NAME);
                    async { NamedFile::open(index_file) }
                }),
            );

        let mut daps_manager = daps_service.lock().expect("Daps manager lock should be acquired");
        daps_manager.load_daps();

        for dap in daps_manager.daps_iter() {
            app = app.configure(dap.http_configure());
        }
        app
    })
    .bind((settings.http.host.as_str(), settings.http.port))?
    .run()
    .await
}
