pub use actix_files;
pub use actix_web;

use std::io;

use actix_files::{Files, NamedFile};
use actix_web::{http, middleware, web, App, HttpResponse, HttpServer};

use self::{
    lapps::{Lapp, LappsProvider},
    settings::Settings,
};

pub mod auth;
pub mod convert;
pub mod error;
pub mod gossipsub;
pub mod handler;
pub mod lapps;
pub mod settings;
pub mod ws;

pub async fn run(settings: Settings) -> io::Result<()> {
    let lapps_path = settings.lapps.path.clone();
    let lapps_provider = web::block(move || LappsProvider::new(lapps_path))
        .await
        .expect("Lapps provider should be constructed")?;
    let web_root = settings.http.web_root.clone();
    let laplace_access_token = settings.http.access_token.clone().unwrap_or_default();

    HttpServer::new(move || {
        let static_dir = web_root.join(Lapp::static_dir_name());
        let laplace_uri = format!("/{}", Lapp::main_name());

        let mut app = App::new()
            .app_data(web::Data::new(lapps_provider.clone()))
            .wrap(middleware::DefaultHeaders::new().header("X-Version", "0.2"))
            .wrap(middleware::NormalizePath::trim())
            .wrap_fn({
                let lapps_provider = lapps_provider.clone();
                let laplace_access_token = laplace_access_token.clone();
                auth::create_check_access_middleware(lapps_provider, laplace_access_token)
            })
            .wrap(middleware::Compress::default())
            .wrap(middleware::Logger::default())
            .service(Files::new(&Lapp::main_static_uri(), &static_dir).index_file(Lapp::index_file_name()))
            .route(
                "/",
                web::route().to({
                    let laplace_uri = laplace_uri.clone();
                    move || {
                        HttpResponse::Found()
                            .append_header((http::header::LOCATION, laplace_uri.as_str()))
                            .finish()
                    }
                }),
            )
            .route(
                &laplace_uri,
                web::get().to(move || {
                    let index_file = static_dir.join(Lapp::index_file_name());
                    async { NamedFile::open(index_file) }
                }),
            )
            .route(&Lapp::main_uri("lapps"), web::get().to(handler::get_lapps))
            .route(&Lapp::main_uri("lapp"), web::post().to(handler::update_lapp));

        let mut lapps_manager = lapps_provider.lock().expect("Lapps manager lock should be acquired");
        lapps_manager.load_lapps();

        for lapp in lapps_manager.lapps_iter() {
            app = app.configure(lapp.http_configure());
        }
        app
    })
    .bind((settings.http.host.as_str(), settings.http.port))?
    .run()
    .await
}