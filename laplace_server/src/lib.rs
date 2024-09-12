use std::io;
use std::net::{IpAddr, SocketAddr};
use std::str::FromStr;
use std::sync::Arc;

use axum::extract::{DefaultBodyLimit, Request};
use axum::http::{HeaderName, HeaderValue};
use axum::response::Redirect;
use axum::routing::get;
use axum::{middleware, Router, ServiceExt};
use axum_server::tls_rustls::RustlsConfig;
use const_format::concatcp;
use flexi_logger::{style, Age, Cleanup, Criterion, DeferredNow, Duplicate, FileSpec, Logger, LoggerHandle, Naming};
use log::Record;
use rustls::ServerConfig;
use tower::{Layer, ServiceBuilder};
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
        .format(custom_colored_detailed_format)
        .start()?;

    Ok(handle)
}

fn custom_colored_detailed_format(
    writer: &mut dyn io::Write,
    now: &mut DeferredNow,
    record: &Record,
) -> Result<(), io::Error> {
    let level = record.level();
    let start_idx = record
        .file()
        .and_then(|path| {
            path.rfind("/src")
                .and_then(|src_idx| path[..src_idx].rfind('/'))
                .map(|start_idx| start_idx + 1)
        })
        .unwrap_or(0);
    let path = record.file().map(|path| &path[start_idx..]).unwrap_or("<unnamed>");

    write!(
        writer,
        "[{}] {} [{}] {}:{}: {}",
        style(level).paint(now.format("%Y-%m-%d %H:%M:%S%.6f").to_string()),
        style(level).paint(level.as_str()),
        record.module_path().unwrap_or("<unnamed>"),
        path,
        record.line().unwrap_or(0),
        style(level).paint(record.args().to_string()),
    )
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

    let root_url = format!(
        "{schema}://{host}:{port}",
        schema = if settings.ssl.enabled { "https" } else { "http" },
        host = settings.http.host,
        port = settings.http.port,
    );
    if settings.http.print_url {
        let access_query = (!laplace_access_token.is_empty())
            .then(|| format!("?access_token={laplace_access_token}"))
            .unwrap_or_default();

        log::info!("Laplace URL: {root_url}/{access_query}",);
    }

    log::info!("Load lapps");
    lapps_provider.read_manager().await.autoload_lapps().await;

    if settings.http.print_url {
        for (lapp_name, lapp_settings) in lapps_provider.read_manager().await.lapp_settings_iter() {
            if lapp_settings.is_lapp_startup_active() {
                let access_query = lapp_settings
                    .application
                    .access_token
                    .as_ref()
                    .map(|access_token| format!("?access_token={access_token}"))
                    .unwrap_or_default();
                log::info!("Lapp '{lapp_name}' URL: {root_url}/{lapp_name}{access_query}");
            }
        }
    }

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
                .layer(DefaultBodyLimit::max(upload_file_limit))
                .layer(CompressionLayer::new())
                .layer(SetResponseHeaderLayer::if_not_present(
                    HeaderName::from_static("x-version"),
                    HeaderValue::from_static(VERSION),
                )),
        )
        .with_state(lapps_provider);
    let service = ServiceExt::<Request>::into_make_service(NormalizePathLayer::trim_trailing_slash().layer(router));

    log::info!("Run HTTP server");
    let http_server_addr = SocketAddr::new(IpAddr::from_str(&settings.http.host)?, settings.http.port);
    if settings.ssl.enabled {
        let (certificates, private_key) = auth::prepare_certificates(
            &settings.ssl.certificate_path,
            &settings.ssl.private_key_path,
            &settings.http.host,
        )?;

        rustls::crypto::ring::default_provider()
            .install_default()
            .expect("Failed to install default provider");
        let config = ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(certificates, private_key)?;

        axum_server::bind_rustls(http_server_addr, RustlsConfig::from_config(Arc::new(config)))
            .serve(service)
            .await?
    } else {
        axum_server::Server::bind(http_server_addr).serve(service).await?
    };

    log::info!("Shutdown the context");
    ctx.shutdown().await;

    Ok(())
}
