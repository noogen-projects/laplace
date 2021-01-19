use std::{io, path::PathBuf};

use actix_files::{Files, NamedFile};
use actix_web::{get, middleware, web, App, HttpResponse, HttpServer};

use self::{
    daps::{Dap, DapsService},
    error::ServerError,
    settings::Settings,
};

mod daps;
mod error;
mod settings;

#[get("/daps")]
async fn get_daps(daps_service: web::Data<DapsService>) -> HttpResponse {
    daps_service.handle_http(|daps_manager| {
        let daps: Vec<_> = daps_manager.daps_iter().filter(|dap| !dap.is_main_client()).collect();
        HttpResponse::Ok().json(daps)
    })
}

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
            )
            .service(get_daps);

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
