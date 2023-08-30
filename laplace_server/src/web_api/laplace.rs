use std::path::PathBuf;

use axum::routing::{get, post};
use axum::Router;
use tower_http::services::{ServeDir, ServeFile};

use crate::lapps::{Lapp, LappsProvider};

pub mod handler;

pub fn router(
    laplace_uri: &'static str,
    static_dir: impl Into<PathBuf>,
    lapps_dir: impl Into<PathBuf>,
) -> Router<LappsProvider> {
    let static_dir = static_dir.into();
    let lapps_dir = lapps_dir.into();

    Router::new()
        .route_service(laplace_uri, ServeFile::new(static_dir.join(Lapp::index_file_name())))
        .nest_service(
            &format!("{laplace_uri}/{}", Lapp::static_dir_name()),
            ServeDir::new(lapps_dir.join(Lapp::main_name()).join(Lapp::static_dir_name())),
        )
        .route(&format!("{laplace_uri}/lapps"), get(handler::get_lapps))
        .route(&format!("{laplace_uri}/lapp/add"), post(handler::add_lapp))
        .route(&format!("{laplace_uri}/lapp/update"), post(handler::update_lapp))
}
