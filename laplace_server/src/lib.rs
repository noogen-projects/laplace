use std::fs;
use std::io::{BufReader, Write};

use actix_easy_multipart::MultipartFormConfig;
use actix_files::{Files, NamedFile};
use actix_web::{http, middleware, web, App, HttpResponse, HttpServer};
use flexi_logger::{Age, Cleanup, Criterion, Duplicate, FileSpec, Logger, LoggerHandle, Naming};
use rustls::{Certificate, PrivateKey, ServerConfig};
use rustls_pemfile::{certs, pkcs8_private_keys};
pub use {actix_files, actix_web};

use self::error::{AppError, AppResult};
use self::lapps::{Lapp, LappsProvider};
use self::settings::{LoggerSettings, Settings};

pub mod auth;
pub mod convert;
pub mod error;
pub mod handler;
pub mod lapps;
pub mod service;
pub mod settings;

pub fn init_logger(settings: &LoggerSettings) -> AppResult<LoggerHandle> {
    let mut logger = Logger::try_with_env_or_str(&settings.spec)?;
    if let Some(path) = &settings.path {
        logger = logger
            .log_to_file(FileSpec::try_from(path)?.suppress_timestamp())
            .rotate(
                Criterion::Age(Age::Day),
                Naming::Timestamps,
                Cleanup::KeepLogFiles(settings.keep_log_for_days),
            )
            .append()
    }
    let handle = logger
        .duplicate_to_stdout(if settings.duplicate_to_stdout {
            Duplicate::All
        } else {
            Duplicate::None
        })
        .use_utc()
        .format(flexi_logger::colored_detailed_format)
        .start()?;

    Ok(handle)
}

pub async fn run(settings: Settings) -> AppResult<()> {
    let web_root = settings.http.web_root.clone();
    let laplace_access_token = auth::prepare_access_token(settings.http.access_token.clone())?;
    let upload_file_limit = settings.http.upload_file_limit;
    let lapps_dir = settings.lapps.path.clone();
    let lapps_provider = LappsProvider::new(&lapps_dir)
        .await
        .expect("Lapps provider should be constructed");

    if settings.http.print_url {
        let access_query = (!laplace_access_token.is_empty())
            .then(|| format!("?access_token={}", laplace_access_token))
            .unwrap_or_default();

        log::info!(
            "Laplace URL: {schema}://{host}:{port}/{access_query}",
            schema = if settings.ssl.enabled { "https" } else { "http" },
            host = settings.http.host,
            port = settings.http.port,
        );
    }

    log::info!("Load lapps");
    lapps_provider.read_manager().await.load_lapps().await;
    let lapps_provider = web::Data::new(lapps_provider);

    log::info!("Create HTTP server");
    let http_server = HttpServer::new(move || {
        let static_dir = web_root.join(Lapp::static_dir_name());
        let laplace_uri = format!("/{}", Lapp::main_name());

        App::new()
            .app_data(web::Data::clone(&lapps_provider))
            .app_data(
                MultipartFormConfig::default()
                    .total_limit(upload_file_limit)
                    .memory_limit(upload_file_limit),
            )
            .wrap(middleware::DefaultHeaders::new().add(("X-Version", "0.2")))
            .wrap(middleware::NormalizePath::trim())
            .wrap(auth::middleware::CheckAccess::new(
                web::Data::clone(&lapps_provider),
                laplace_access_token,
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
        let (certificates, private_key) = auth::prepare_certificates(
            &settings.ssl.certificate_path,
            &settings.ssl.private_key_path,
            &settings.http.host,
        )?;

        let config = ServerConfig::builder()
            .with_safe_defaults()
            .with_no_client_auth()
            .with_single_cert(certificates, private_key)?;

        http_server.bind_rustls(http_server_addr, config)?
    } else {
        http_server.bind(http_server_addr)?
    };

    log::info!("Run HTTP server");
    http_server.run().await.map_err(Into::into)
}
