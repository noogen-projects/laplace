use actix_web::{HttpResponse, ResponseError};
use thiserror::Error;
use wasmer::{ExportError, RuntimeError};

#[derive(Debug, Error)]
pub enum ServerError {
    #[error("Daps service poisoned lock: another task failed inside")]
    DapsServiceNotLock,

    #[error("Dap '{0}' is not loaded")]
    DapNotLoaded(String),

    #[error("Dap export error: {0}")]
    DapExportFail(#[from] ExportError),

    #[error("Dap runtime error: {0}")]
    DapRuntimeFail(#[from] RuntimeError),

    #[error("Wasm result value cannot be parsed")]
    ResultNotParsed,
}

impl ServerError {
    pub fn into_http_response(self) -> HttpResponse {
        HttpResponse::from_error(self.into())
    }
}

impl ResponseError for ServerError {}

pub type ServerResult<T> = Result<T, ServerError>;
