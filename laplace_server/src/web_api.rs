use axum::http::StatusCode;
use axum::Json;
use serde_json::{json, Value};

use crate::error::{ServerError, ServerResult};

pub mod laplace;
pub mod lapp;

pub type JsonErrResponse = (StatusCode, Json<Value>);
pub type ResultResponse<T> = Result<T, JsonErrResponse>;

pub trait IntoJsonResponse {
    type Output;

    fn into_json_response(self) -> ResultResponse<Json<Self::Output>>;
}

impl<T> IntoJsonResponse for ServerResult<T> {
    type Output = T;

    fn into_json_response(self) -> ResultResponse<Json<Self::Output>> {
        self.map(Json).map_err(err_into_json_response)
    }
}

pub fn err_into_json_response(err: ServerError) -> JsonErrResponse {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(json!({ "error": err.to_string() })),
    )
}
