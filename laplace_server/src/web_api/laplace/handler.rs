use std::io;

use axum::extract::State;
use axum::response::{IntoResponse, Response};
use axum::Json;
use axum_typed_multipart::{FieldData, TryFromMultipart, TypedMultipart};
use tempfile::NamedTempFile;
use zip::ZipArchive;

use crate::error::{ServerError, ServerResult};
use crate::lapps::{CommonLappGuard, CommonLappResponse, Lapp, LappUpdateRequest, LappsProvider};
use crate::web_api::err_into_json_response;

pub async fn get_lapps(State(lapps_provider): State<LappsProvider>) -> impl IntoResponse {
    process_get_lapps(lapps_provider).await.map_err(err_into_json_response)
}

#[derive(TryFromMultipart)]
pub struct LarUpload {
    // This field will be limited to the total size of the request body.
    #[form_data(limit = "unlimited")]
    pub lar: FieldData<NamedTempFile>,
}

pub async fn add_lapp(
    State(lapps_provider): State<LappsProvider>,
    TypedMultipart(form): TypedMultipart<LarUpload>,
) -> impl IntoResponse {
    process_add_lapp(lapps_provider, form.lar)
        .await
        .map_err(err_into_json_response)
}

pub async fn update_lapp(
    State(lapps_provider): State<LappsProvider>,
    Json(update_request): Json<LappUpdateRequest>,
) -> impl IntoResponse {
    process_update_lapp(lapps_provider, update_request)
        .await
        .map_err(err_into_json_response)
}

async fn process_get_lapps(lapps_provider: LappsProvider) -> ServerResult<Response> {
    let manager = lapps_provider.read_manager().await;

    let mut lapps = Vec::new();
    for (lapp_name, lapp_settings) in manager.lapp_settings_iter() {
        if !Lapp::is_main(lapp_name) {
            lapps.push(CommonLappGuard(lapp_settings));
        }
    }
    lapps.sort_unstable_by(|lapp_a, lapp_b| lapp_a.name().cmp(lapp_b.name()));

    Ok(Json(CommonLappResponse::lapps(lapps)).into_response())
}

async fn process_add_lapp(lapps_provider: LappsProvider, lar: FieldData<NamedTempFile>) -> ServerResult<Response> {
    let file_name = lar.metadata.file_name.ok_or(ServerError::UnknownLappName)?;
    let lapp_name = file_name
        .strip_suffix(".zip")
        .unwrap_or_else(|| file_name.strip_suffix(".lar").unwrap_or(&file_name));

    extract_lar(&lapps_provider, lapp_name, ZipArchive::new(lar.contents.as_file())?).await?;
    lapps_provider.write_manager().await.insert_lapp_settings(lapp_name);

    process_get_lapps(lapps_provider).await
}

async fn extract_lar<R: io::Read + io::Seek>(
    lapps_provider: &LappsProvider,
    lapp_name: &str,
    mut archive: ZipArchive<R>,
) -> ServerResult<()> {
    let lapp_dir = lapps_provider.read_manager().await.lapp_dir(lapp_name);

    if lapp_dir.exists() {
        if !lapp_dir.is_dir() {
            return Err(ServerError::WrongLappDirectory(lapp_dir.display().to_string()));
        }

        if lapp_dir.read_dir()?.next().is_some() {
            return Err(ServerError::LappAlreadyExists(lapp_name.into()));
        }
    }

    archive.extract(lapp_dir).map_err(Into::into)
}

async fn process_update_lapp(
    lapps_provider: LappsProvider,
    update_request: LappUpdateRequest,
) -> ServerResult<Response> {
    let update_query = update_request.into_query();
    let updated = lapps_provider
        .write_manager()
        .await
        .update_lapp_settings(update_query)
        .await?;

    Ok(Json(CommonLappResponse::Updated { updated }).into_response())
}
