use std::io;

pub use actix_files;
use actix_files::{Files, NamedFile};
pub use actix_web;
use actix_web::{middleware, web, App, HttpServer};

use self::{
    daps::{Dap, DapsService},
    settings::Settings,
};

pub mod daps;
pub mod error;
pub mod handler;
pub mod settings;
pub mod ws;

pub async fn run(settings: Settings) -> io::Result<()> {
    let daps_service = DapsService::new(&settings.daps.path)?;
    let web_root = settings.http.web_root.clone();

    HttpServer::new(move || {
        let static_dir = web_root.join(Dap::static_dir_name());

        let mut app = App::new()
            .data(daps_service.clone())
            .wrap(middleware::DefaultHeaders::new().header("X-Version", "0.2"))
            .wrap(middleware::NormalizePath::default())
            .wrap(middleware::Compress::default())
            .wrap(middleware::Logger::default())
            .service(Files::new(&Dap::main_static_uri(), &static_dir).index_file(Dap::index_file_name()))
            .route(
                "/",
                web::get().to(move || {
                    let index_file = static_dir.join(Dap::index_file_name());
                    async { NamedFile::open(index_file) }
                }),
            )
            .route(&Dap::main_uri("daps"), web::get().to(handler::get_daps))
            .route(&Dap::main_uri("dap"), web::post().to(handler::update_dap));

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
