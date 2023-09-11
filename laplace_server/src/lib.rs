use std::net::{IpAddr, SocketAddr};
use std::str::FromStr;
use std::sync::Arc;

use axum::extract::DefaultBodyLimit;
use axum::http::{HeaderName, HeaderValue};
use axum::response::Redirect;
use axum::routing::get;
use axum::{middleware, Router};
use axum_server::tls_rustls::RustlsConfig;
use const_format::concatcp;
use flexi_logger::{Age, Cleanup, Criterion, Duplicate, FileSpec, Logger, LoggerHandle, Naming};
use rustls::ServerConfig;
use tower::ServiceBuilder;
use tower_http::compression::CompressionLayer;
use tower_http::normalize_path::NormalizePathLayer;
use tower_http::services::{ServeDir, ServeFile};
use tower_http::set_header::SetResponseHeaderLayer;
use truba::Context;

use crate::error::AppResult;
use crate::lapps::{Lapp, LappsProvider};
use crate::service::Addr;
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
    let ctx = Context::<Addr>::default();
    let lapps_provider = LappsProvider::new(&settings.lapps, ctx.clone())
        .await
        .unwrap_or_else(|err| {
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

    log::info!("Create HTTP server");
    let static_dir = web_root.join(Lapp::static_dir_name());
    let laplace_uri = concatcp!("/", Lapp::main_name());

    let router = Router::new()
        .route("/", get(|| async { Redirect::to(laplace_uri) }))
        .route_service("/favicon.ico", ServeFile::new(static_dir.join("favicon.ico")))
        .nest_service(&Lapp::main_static_uri(), ServeDir::new(&static_dir))
        .fallback_service(ServeFile::new(Lapp::index_file_name()))
        .merge(web_api::laplace::router(laplace_uri, &static_dir, &settings.lapps.path))
        .merge(web_api::lapp::router())
        .route_layer(middleware::from_fn_with_state(
            (lapps_provider.clone(), laplace_access_token),
            auth::middleware::check_access,
        ))
        .layer(
            ServiceBuilder::new()
                .layer(NormalizePathLayer::trim_trailing_slash())
                .layer(DefaultBodyLimit::max(upload_file_limit))
                .layer(CompressionLayer::new())
                .layer(SetResponseHeaderLayer::if_not_present(
                    HeaderName::from_static("x-version"),
                    HeaderValue::from_static(VERSION),
                )),
        )
        .with_state(lapps_provider);

    log::info!("Run HTTP server");
    let http_server_addr = SocketAddr::new(IpAddr::from_str(&settings.http.host)?, settings.http.port);
    if settings.ssl.enabled {
        let (certificates, private_key) = auth::prepare_certificates(
            &settings.ssl.certificate_path,
            &settings.ssl.private_key_path,
            &settings.http.host,
        )?;

        let config = ServerConfig::builder()
            .with_safe_defaults()
            .with_no_client_auth()
            .with_single_cert(certificates, private_key)?;

        axum_server::bind_rustls(http_server_addr, RustlsConfig::from_config(Arc::new(config)))
            .serve(router.into_make_service())
            .await?
    } else {
        axum::Server::bind(&http_server_addr)
            .serve(router.into_make_service())
            .await?
    };

    log::info!("Shutdown the context");
    ctx.shutdown().await;

    Ok(())
}
