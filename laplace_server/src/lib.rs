use actix_easy_multipart::MultipartFormConfig;
use actix_files::{Files, NamedFile};
use actix_web::{http, middleware, web, App, HttpResponse, HttpServer};
use const_format::concatcp;
use flexi_logger::{Age, Cleanup, Criterion, Duplicate, FileSpec, Logger, LoggerHandle, Naming};
use rustls::ServerConfig;
pub use {actix_files, actix_web};

use crate::error::AppResult;
use crate::lapps::{Lapp, LappsProvider};
use crate::settings::{LoggerSettings, Settings};

pub mod auth;
pub mod convert;
pub mod error;
pub mod lapps;
pub mod service;
pub mod settings;
pub mod web_api;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

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
    let lapps_provider = LappsProvider::new(&settings.lapps).await.unwrap_or_else(|err| {
        panic!(
            "Lapps provider should be constructed from settings {:?}: {err}",
            settings.lapps
        )
    });

    if settings.http.print_url {
        let access_query = (!laplace_access_token.is_empty())
            .then(|| format!("?access_token={laplace_access_token}"))
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
        let laplace_uri = concatcp!("/", Lapp::main_name());
        let route_file_path = concatcp!("/", Lapp::static_dir_name(), "/{file_path:.*}");

        App::new()
            .app_data(web::Data::clone(&lapps_provider))
            .app_data(
                MultipartFormConfig::default()
                    .total_limit(upload_file_limit)
                    .memory_limit(upload_file_limit),
            )
            .wrap(middleware::DefaultHeaders::new().add(("X-Version", VERSION)))
            .wrap(middleware::NormalizePath::trim())
            .wrap(auth::middleware::CheckAccess::new(
                web::Data::clone(&lapps_provider),
                laplace_access_token,
            ))
            .wrap(middleware::Compress::default())
            .wrap(middleware::Logger::default())
            .route(
                "/favicon.ico",
                web::get().to({
                    let favicon_path = static_dir.join("favicon.ico");
                    move || {
                        let favicon_path = favicon_path.clone();
                        async move { NamedFile::open(favicon_path) }
                    }
                }),
            )
            .service(Files::new(&Lapp::main_static_uri(), &static_dir).index_file(Lapp::index_file_name()))
            .route(
                "/",
                web::route().to(move || {
                    let redirect = HttpResponse::Found()
                        .append_header((http::header::LOCATION, laplace_uri))
                        .finish();
                    async { redirect }
                }),
            )
            .service(web_api::laplace::services(
                laplace_uri,
                &static_dir,
                &settings.lapps.path,
                route_file_path,
            ))
            .service(web_api::lapp::services(route_file_path))
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
