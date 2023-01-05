use std::io;

use actix_easy_multipart::{tempfile::Tempfile, MultipartForm};
use actix_web::{web, HttpResponse};
use zip::ZipArchive;

use crate::{
    error::{ServerError, ServerResult},
    lapps::{CommonLappGuard, CommonLappResponse, LappUpdateRequest, LappsProvider},
};

pub async fn get_lapps(lapps_service: web::Data<LappsProvider>) -> HttpResponse {
    lapps_service
        .into_inner()
        .handle(|lapps_provider| async { process_get_lapps(lapps_provider) })
        .await
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

fn process_get_lapps(lapps_provider: LappsProvider) -> ServerResult<HttpResponse> {
    lapps_provider.read_manager().and_then(|manager| {
        manager
            .lapps_iter()
            .filter_map(|lapp| match lapp.read() {
                Ok(lapp) if lapp.is_main() => None,
                Ok(lapp) => Some(Ok(CommonLappGuard(lapp))),
                Err(_) => Some(Err(ServerError::LappNotLock)),
            })
            .collect::<ServerResult<Vec<_>>>()
            .map(|mut lapps| {
                lapps.sort_unstable_by(|lapp_a, lapp_b| lapp_a.name().cmp(lapp_b.name()));
                HttpResponse::Ok().json(CommonLappResponse::lapps(lapps))
            })
    })
}

async fn process_add_lapp(lapps_provider: LappsProvider, lar: Tempfile) -> ServerResult<HttpResponse> {
    let file_name = lar.file_name.as_ref().ok_or(ServerError::UnknownLappName)?;
    let lapp_name = file_name
        .strip_suffix(".zip")
        .unwrap_or_else(|| file_name.strip_suffix(".lar").unwrap_or(&file_name));

    extract_lar(&lapps_provider, lapp_name, ZipArchive::new(&lar.file)?)?;

    lapps_provider.write_manager().and_then(|mut manager| {
        manager.insert_lapp(lapp_name);
        let lapp = manager.lapp_mut(lapp_name)?;
        if lapp.enabled() {
            manager.load(lapp)?;
        }
        Ok(())
    })?;

    process_get_lapps(lapps_provider)
}

fn extract_lar<R: io::Read + io::Seek>(
    lapps_provider: &LappsProvider,
    lapp_name: &str,
    mut archive: ZipArchive<R>,
) -> ServerResult<()> {
    let lapp_dir = lapps_provider.read_manager()?.lapp_dir(lapp_name);

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
    let manager = lapps_provider.read_manager()?;
    let mut lapp = manager.lapp_mut(&update_query.lapp_name)?;

    let updated = lapp.update(update_query)?;
    if updated.enabled.is_some() {
        if lapp.enabled() {
            manager.load(lapp)?;
        } else {
            manager.unload(lapp).await?;
        }
    }
    Ok(HttpResponse::Ok().json(CommonLappResponse::Updated { updated }))
}
