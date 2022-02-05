use std::{fmt, io};

use actix_web::{HttpResponse, ResponseError};
use openssl::error::ErrorStack;
use rcgen::RcgenError;
use rusqlite::Error as SqlError;
use thiserror::Error;
use wasmer::{CompileError, ExportError, InstantiationError, RuntimeError};
use wasmer_wasi::{WasiError, WasiStateCreationError};

use laplace_common::lapp::Permission;

use crate::{
    lapps::{LappInstanceError, LappSettingsError},
    service::gossipsub,
};

pub type AppResult<T> = Result<T, AppError>;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("IO error: {0}")]
    IoError(#[from] io::Error),

    #[error("SSL error: {0:?}")]
    ErrorStack(#[from] ErrorStack),

    #[error("Certificate generation error: {0:?}")]
    RcgenError(#[from] RcgenError),
}

pub type ServerResult<T> = Result<T, ServerError>;

#[derive(Debug, Error)]
pub enum ServerError {
    #[error("Web error: {0}")]
    WebError(#[from] actix_web::Error),

    #[error("P2p error: {0}")]
    P2pError(#[from] gossipsub::Error),

    #[error("Wrong parse JSON: {0}")]
    ParseJsonError(#[from] serde_json::Error),

    #[error("Lapps service poisoned lock: another task failed inside")]
    LappsServiceNotLock,

    #[error("Lapps poisoned lock")]
    LappNotLock,

    #[error("Lapp '{0}' does not exist")]
    LappNotFound(String),

    #[error("Lapp '{0}' is not enabled")]
    LappNotEnabled(String),

    #[error("Lapp '{0}' is not loaded")]
    LappNotLoaded(String),

    #[error("Permission '{}' denied for lapp '{0}'", .1.as_str())]
    LappPermissionDenied(String, Permission),

    #[error("Lapp export error: {0}")]
    LappExportFail(#[from] ExportError),

    #[error("Lapp runtime error: {0}")]
    LappRuntimeFail(#[from] RuntimeError),

    #[error("Lapp settings operation error: {0}")]
    LappSettingsFail(#[from] LappSettingsError),

    #[error("Lapp file operation error: {0}")]
    LappIoError(#[from] io::Error),

    #[error("Lapp compile error: {0}")]
    LappCompileFail(#[from] CompileError),

    #[error("Lapp WASI state creation error: {0}")]
    LappWasiCreationFail(#[from] WasiStateCreationError),

    #[error("Lapp WASI error: {0}")]
    LappWasi(#[from] WasiError),

    #[error("Lapp instantiate error: {0}")]
    LappInstantiateFail(#[from] InstantiationError),

    #[error("Wasm result value has wrong data length")]
    WrongResultLength,

    #[error("Wasm result value cannot be parsed")]
    ResultNotParsed,

    #[error("Lapp instance operation error: {0}")]
    LappInstanceFail(#[from] LappInstanceError),

    #[error("Lapp database operation error: {0:?}")]
    LappDatabaseError(#[from] SqlError),

    #[error("Lapp initialization error: {0:?}")]
    LappInitError(String),

    #[error("Blocking call error: {0}")]
    BlockingError(#[from] actix_web::error::BlockingError),
}

impl ResponseError for ServerError {}

impl From<ServerError> for HttpResponse {
    fn from(error: ServerError) -> Self {
        error_response(error)
    }
}

pub fn error_response(err: impl fmt::Debug) -> HttpResponse {
    let error_message = format!("{:#?}", err);
    log::error!("Internal Server error: {}", error_message);

    HttpResponse::InternalServerError().body(error_message)
}
