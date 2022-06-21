pub use actix_files;
pub use actix_web;

use std::{fs::File, io::Write};

use actix_easy_multipart::extractor::MultipartFormConfig;
use actix_files::{Files, NamedFile};
use actix_web::{http, middleware, web, App, HttpResponse, HttpServer};
use openssl::ssl::{SslAcceptor, SslFiletype, SslMethod};

use self::{
    error::AppResult,
    lapps::{Lapp, LappsProvider},
    settings::Settings,
};

pub mod auth;
pub mod convert;
pub mod error;
pub mod handler;
pub mod lapps;
pub mod service;
pub mod settings;

pub async fn run(settings: Settings) -> AppResult<()> {
    let lapps_path = settings.lapps.path.clone();
    let lapps_provider = web::Data::new(
        web::block(move || LappsProvider::new(lapps_path))
            .await
            .expect("Lapps provider should be constructed")?,
    );
    let web_root = settings.http.web_root.clone();
    let laplace_access_token = settings.http.access_token.clone().unwrap_or_default();
    let upload_file_limit = settings.http.upload_file_limit;

    if settings.http.print_url {
        let access_query = if !laplace_access_token.is_empty() {
            format!("?access_token={}", laplace_access_token)
        } else {
            "".into()
        };
        log::info!(
            "Laplace URL: {}://{}:{}/{}",
            if settings.ssl.enabled { "https" } else { "http" },
            settings.http.host,
            settings.http.port,
            access_query
        );
    }

    let http_server = HttpServer::new(move || {
        let static_dir = web_root.join(Lapp::static_dir_name());
        let laplace_uri = format!("/{}", Lapp::main_name());

        let mut app = App::new()
            .app_data(web::Data::clone(&lapps_provider))
            .app_data(MultipartFormConfig::default().file_limit(upload_file_limit))
            .wrap(middleware::DefaultHeaders::new().add(("X-Version", "0.2")))
            .wrap(middleware::NormalizePath::trim())
            .wrap_fn(auth::create_check_access_middleware(
                web::Data::clone(&lapps_provider),
                laplace_access_token.clone(),
            ))
            .wrap(middleware::Compress::default())
            .wrap(middleware::Logger::default())
            .service(Files::new(&Lapp::main_static_uri(), &static_dir).index_file(Lapp::index_file_name()))
            .route(
                "/",
                web::route().to({
                    let laplace_uri = laplace_uri.clone();
                    move || {
                        let redirect = HttpResponse::Found()
                            .append_header((http::header::LOCATION, laplace_uri.as_str()))
                            .finish();
                        async { redirect }
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
            .route(&Lapp::main_uri("lapp/add"), web::post().to(handler::add_lapp))
            .route(&Lapp::main_uri("lapp/update"), web::post().to(handler::update_lapp));

        let lapps_manager = lapps_provider.read_manager().expect("Lapps is not locked");
        lapps_manager.load_lapps();
        for lapp in lapps_manager.lapps_iter() {
            app = app.configure(lapp.read().expect("Lapp is not readable").http_configure());
        }
        app
    });

    let http_server_addr = (settings.http.host.as_str(), settings.http.port);
    let http_server = if settings.ssl.enabled {
        let private_key_path = &settings.ssl.private_key_path;
        let certificate_path = &settings.ssl.certificate_path;

        if !certificate_path.exists() && !private_key_path.exists() {
            let certificate = rcgen::generate_simple_self_signed(vec![settings.http.host.clone()])?;

            File::create(private_key_path)?.write_all(certificate.serialize_private_key_pem().as_bytes())?;
            File::create(certificate_path)?.write_all(certificate.serialize_pem()?.as_bytes())?;
        }

        let mut ssl_builder = SslAcceptor::mozilla_intermediate(SslMethod::tls())?;
        ssl_builder.set_private_key_file(private_key_path, SslFiletype::PEM)?;
        ssl_builder.set_certificate_chain_file(certificate_path)?;

        http_server.bind_openssl(http_server_addr, ssl_builder)?
    } else {
        http_server.bind(http_server_addr)?
    };

    Ok(http_server.run().await?)
}
