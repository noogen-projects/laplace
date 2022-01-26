use std::sync::Arc;

use actix_web::{web, HttpResponse};

use crate::{
    error::{ServerError, ServerResult},
    lapps::{CommonLappGuard, CommonLappResponse, LappUpdateRequest, LappsManager, LappsProvider},
};

pub async fn get_lapps(lapps_service: web::Data<LappsProvider>) -> HttpResponse {
    lapps_service
        .into_inner()
        .handle(|lapps_manager| {
            let result = lapps_manager
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
                });

            async { result }
        })
        .await
}

pub async fn update_lapp(lapps_service: web::Data<LappsProvider>, body: String) -> HttpResponse {
    lapps_service
        .into_inner()
        .handle(move |lapps_manager| async move { process_update_lapp(lapps_manager, body).await })
        .await
}

async fn process_update_lapp(lapps_manager: Arc<LappsManager>, body: String) -> ServerResult<HttpResponse> {
    let request: LappUpdateRequest = serde_json::from_str(&body)?;
    let update_query = request.into_query();
    let mut lapp = lapps_manager.lapp_mut(&update_query.lapp_name)?;

    let updated = lapp.update(update_query)?;
    if updated.enabled.is_some() {
        if lapp.enabled() {
            lapps_manager.load(lapp.name())?;
        } else {
            lapps_manager.unload(lapp.name()).await?;
        }
    }
    Ok(HttpResponse::Ok().json(CommonLappResponse::Updated { updated }))
}
