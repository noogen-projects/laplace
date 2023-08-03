use std::io;

use actix_easy_multipart::tempfile::Tempfile;
use actix_easy_multipart::MultipartForm;
use actix_web::{web, HttpResponse};
use zip::ZipArchive;

use crate::error::{ServerError, ServerResult};
use crate::lapps::{CommonLappGuard, CommonLappResponse, LappUpdateRequest, LappsProvider};

pub async fn get_lapps(lapps_service: web::Data<LappsProvider>) -> HttpResponse {
    lapps_service.into_inner().handle(process_get_lapps).await
}

#[derive(MultipartForm)]
pub struct LarUpload {
    pub lar: Tempfile,
}

pub async fn add_lapp(lapps_service: web::Data<LappsProvider>, form: MultipartForm<LarUpload>) -> HttpResponse {
    lapps_service
        .into_inner()
        .handle(move |lapps_provider| async move { process_add_lapp(lapps_provider, form.into_inner().lar).await })
        .await
}

pub async fn update_lapp(lapps_service: web::Data<LappsProvider>, body: String) -> HttpResponse {
    lapps_service
        .into_inner()
        .handle(move |lapps_provider| async move { process_update_lapp(lapps_provider, body).await })
        .await
}

async fn process_get_lapps(lapps_provider: LappsProvider) -> ServerResult<HttpResponse> {
    let manager = lapps_provider.read_manager().await;

    let mut lapps = Vec::new();
    for lapp in manager.lapps_iter() {
        let lapp = lapp.read().await;
        if !lapp.is_main() {
            lapps.push(CommonLappGuard(lapp));
        }
    }
    lapps.sort_unstable_by(|lapp_a, lapp_b| lapp_a.name().cmp(lapp_b.name()));

    Ok(HttpResponse::Ok().json(CommonLappResponse::lapps(lapps)))
}

async fn process_add_lapp(lapps_provider: LappsProvider, lar: Tempfile) -> ServerResult<HttpResponse> {
    let file_name = lar.file_name.as_ref().ok_or(ServerError::UnknownLappName)?;
    let lapp_name = file_name
        .strip_suffix(".zip")
        .unwrap_or_else(|| file_name.strip_suffix(".lar").unwrap_or(file_name));

    extract_lar(&lapps_provider, lapp_name, ZipArchive::new(&lar.file)?).await?;

    {
        let mut manager = lapps_provider.write_manager().await;
        manager.insert_lapp(lapp_name);

        let shared_lapp = manager.lapp(lapp_name)?;
        let lapp = shared_lapp.write().await;
        if lapp.enabled() {
            manager.load(lapp).await?;
        }
    }

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

async fn process_update_lapp(lapps_provider: LappsProvider, body: String) -> ServerResult<HttpResponse> {
    let request: LappUpdateRequest = serde_json::from_str(&body)?;
    let update_query = request.into_query();
    let manager = lapps_provider.read_manager().await;
    let shared_lapp = manager.lapp(&update_query.lapp_name)?;
    let mut lapp = shared_lapp.write().await;

    let updated = lapp.update(update_query)?;
    if updated.enabled.is_some() {
        if lapp.enabled() {
            manager.load(lapp).await?;
        } else {
            manager.unload(lapp).await?;
        }
    }
    Ok(HttpResponse::Ok().json(CommonLappResponse::Updated { updated }))
}
