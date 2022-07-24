pub use actix_files;
pub use actix_web;

use std::{
    fs,
    io::{BufReader, Write},
};

use actix_easy_multipart::extractor::MultipartFormConfig;
use actix_files::{Files, NamedFile};
use actix_web::{http, middleware, web, App, HttpResponse, HttpServer};
use flexi_logger::{Duplicate, FileSpec, Logger};
use rustls::{Certificate, PrivateKey, ServerConfig};
use rustls_pemfile::{certs, pkcs8_private_keys};

use self::{
    error::{AppError, AppResult},
    lapps::{Lapp, LappsProvider},
    settings::{LoggerSettings, Settings},
};

pub mod auth;
pub mod convert;
pub mod error;
pub mod handler;
pub mod lapps;
pub mod service;
pub mod settings;

pub fn init_logger(settings: &LoggerSettings) {
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
        .format(flexi_logger::colored_detailed_format)
        .start()
        .expect("Logger should be started");
}

pub async fn run(settings: Settings) -> AppResult<()> {
    let lapps_dir = settings.lapps.path.clone();
    let lapps_provider = web::Data::new({
        let lapps_dir = lapps_dir.clone();
        web::block(move || LappsProvider::new(lapps_dir))
            .await
            .expect("Lapps provider should be constructed")?
    });
    let web_root = settings.http.web_root.clone();
    let laplace_access_token = if let Some(access_token) = settings.http.access_token.clone() {
        access_token
    } else {
        auth::generate_token()?
    };
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

    log::info!("Load lapps");
    lapps_provider.read_manager().expect("Lapps is not locked").load_lapps();

    log::info!("Create HTTP server");
    let http_server = HttpServer::new(move || {
        let static_dir = web_root.join(Lapp::static_dir_name());
        let laplace_uri = format!("/{}", Lapp::main_name());

        App::new()
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
            .service(
                web::scope(&laplace_uri)
                    .service(web::resource(["", "/"]).route(web::get().to(move || {
                        let index_file = static_dir.join(Lapp::index_file_name());
                        async { NamedFile::open(index_file) }
                    })))
                    .route(
                        &format!("/{}/{{file_path:.*}}", Lapp::static_dir_name()),
                        web::get().to({
                            let lapps_dir = lapps_dir.clone();
                            move |file_path: web::Path<String>, request| {
                                let file_path = lapps_dir
                                    .join(Lapp::main_name())
                                    .join(Lapp::static_dir_name())
                                    .join(&*file_path);

                                async move { NamedFile::open(file_path).map(|file| file.into_response(&request)) }
                            }
                        }),
                    )
                    .route("/lapps", web::get().to(handler::get_lapps))
                    .route("/lapp/add", web::post().to(handler::add_lapp))
                    .route("/lapp/update", web::post().to(handler::update_lapp)),
            )
            .service(
                web::scope("/{lapp_name}")
                    .service(
                        web::resource(["", "/"]).route(web::get().to(move |lapps_service, lapp_name, request| {
                            lapps::handler::index_file(lapps_service, lapp_name, request)
                        })),
                    )
                    .route(
                        &format!("/{}/{{file_path:.*}}", Lapp::static_dir_name()),
                        web::get().to({
                            move |lapps_service, path: web::Path<(String, String)>, request| {
                                let (lapp_name, file_path) = path.into_inner();
                                lapps::handler::static_file(lapps_service, lapp_name, file_path, request)
                            }
                        }),
                    )
                    .route(
                        "/ws",
                        web::get().to(move |lapps_service, lapp_name, request, stream| {
                            lapps::handler::ws_start(lapps_service, lapp_name, request, stream)
                        }),
                    )
                    .route(
                        "/p2p",
                        web::post().to(move |lapps_service, lapp_name, request| {
                            lapps::handler::gossipsub_start(lapps_service, lapp_name, request)
                        }),
                    )
                    .route(
                        "/{tail}*",
                        web::route().to(move |lapps_service, path: web::Path<(String, String)>, request, body| {
                            let (lapp_name, _tail) = path.into_inner();
                            lapps::handler::http(lapps_service, lapp_name, request, body)
                        }),
                    ),
            )
    });

    let http_server_addr = (settings.http.host.as_str(), settings.http.port);
    let http_server = if settings.ssl.enabled {
        let private_key_path = &settings.ssl.private_key_path;
        let certificate_path = &settings.ssl.certificate_path;

        if !certificate_path.exists() && !private_key_path.exists() {
            log::info!("Generate SSL certificate");
            let certificate = auth::generate_self_signed_certificate(vec![settings.http.host.clone()])?;

            if let Some(parent) = private_key_path.parent() {
                fs::create_dir_all(parent)?;
            }
            if let Some(parent) = certificate_path.parent() {
                fs::create_dir_all(parent)?;
            }

            fs::File::create(private_key_path)?.write_all(certificate.serialize_private_key_pem().as_bytes())?;
            fs::File::create(certificate_path)?.write_all(certificate.serialize_pem()?.as_bytes())?;
        }

        log::info!("Bind SSL");
        let certificates = certs(&mut BufReader::new(fs::File::open(certificate_path)?))?
            .into_iter()
            .map(|buf| Certificate(buf))
            .collect();

        let private_key = PrivateKey(
            pkcs8_private_keys(&mut BufReader::new(fs::File::open(private_key_path)?))?
                .into_iter()
                .next()
                .ok_or(AppError::MissingPrivateKey)?,
        );

        let config = ServerConfig::builder()
            .with_safe_defaults()
            .with_no_client_auth()
            .with_single_cert(certificates, private_key)?;

        http_server.bind_rustls(http_server_addr, config)?
    } else {
        http_server.bind(http_server_addr)?
    };

    log::info!("Run HTTP server");
    Ok(http_server.run().await?)
}
