use std::io;

use actix_web::ResponseError;
use dapla_common::dap::Permission;
use thiserror::Error;
use wasmer::{CompileError, ExportError, InstantiationError, RuntimeError};
use wasmer_wasi::{WasiError, WasiStateCreationError};

use crate::daps::{DapInstanceError, DapSettingsError};

pub type ServerResult<T> = Result<T, ServerError>;

#[derive(Debug, Error)]
pub enum ServerError {
    #[error("Web error: {0}")]
    WebError(#[from] actix_web::Error),

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
}

impl ResponseError for ServerError {}
