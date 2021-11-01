use std::{fmt, io};

use actix_web::{HttpResponse, ResponseError};
use dapla_common::dap::Permission;
use log::error;
use rusqlite::Error as SqlError;
use thiserror::Error;
use wasmer::{CompileError, ExportError, InstantiationError, RuntimeError};
use wasmer_wasi::{WasiError, WasiStateCreationError};

use crate::{
    daps::{DapInstanceError, DapSettingsError},
    gossipsub,
};

pub type ServerResult<T> = Result<T, ServerError>;

#[derive(Debug, Error)]
pub enum ServerError {
    #[error("Web error: {0}")]
    WebError(#[from] actix_web::Error),

    #[error("P2p error: {0}")]
    P2pError(#[from] gossipsub::Error),

    #[error("Wrong parse JSON: {0}")]
    ParseJsonError(#[from] serde_json::Error),

    #[error("Daps service poisoned lock: another task failed inside")]
    DapsServiceNotLock,

    #[error("Dap '{0}' does not exist")]
    DapNotFound(String),

    #[error("Dap '{0}' is not enabled")]
    DapNotEnabled(String),

    #[error("Dap '{0}' is not loaded")]
    DapNotLoaded(String),

    #[error("Permission '{}' denied for dap '{0}'", .1.as_str())]
    DapPermissionDenied(String, Permission),

    #[error("Dap export error: {0}")]
    DapExportFail(#[from] ExportError),

    #[error("Dap runtime error: {0}")]
    DapRuntimeFail(#[from] RuntimeError),

    #[error("Dap settings operation error: {0}")]
    DapSettingsFail(#[from] DapSettingsError),

    #[error("Dap file operation error: {0}")]
    DapIoError(#[from] io::Error),

    #[error("Dap compile error: {0}")]
    DapCompileFail(#[from] CompileError),

    #[error("Dap WASI state creation error: {0}")]
    DapWasiCreationFail(#[from] WasiStateCreationError),

    #[error("Dap WASI error: {0}")]
    DapWasi(#[from] WasiError),

    #[error("Dap instantiate error: {0}")]
    DapInstantiateFail(#[from] InstantiationError),

    #[error("Wasm result value has wrong data length")]
    WrongResultLength,

    #[error("Wasm result value cannot be parsed")]
    ResultNotParsed,

    #[error("Dap instance operation error: {0}")]
    DapInstanceFail(#[from] DapInstanceError),

    #[error("Dap database operation error: {0:?}")]
    DapDatabaseError(#[from] SqlError),

    #[error("Dap initialization error: {0:?}")]
    DapInitError(String),

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
    error!("Internal Server error: {}", error_message);
    HttpResponse::InternalServerError().body(error_message)
}
