use std::{borrow::Cow, ops::Deref};

use actix_web::{web, HttpResponse};

use crate::{
    error::ServerResult,
    lapps::{LappResponse, LappUpdateRequest, LappsManager, LappsProvider},
};

pub async fn get_lapps(lapps_service: web::Data<LappsProvider>) -> HttpResponse {
    lapps_service
        .into_inner()
        .handle(|lapps_manager| {
            let mut lapps: Vec<_> = lapps_manager
                .lapps_iter()
                .filter(|lapp| !lapp.is_main())
                .map(|lapp| Cow::Borrowed(lapp.deref()))
                .collect();
            lapps.sort_unstable_by(|a, b| a.name().cmp(b.name()));
            let response = HttpResponse::Ok().json(LappResponse::Lapps(lapps));

            async { Ok(response) }
        })
        .await
}

pub async fn update_lapp(lapps_service: web::Data<LappsProvider>, body: String) -> HttpResponse {
    lapps_service
        .into_inner()
        .handle(|lapps_manager| {
            let result = update_lapp_handler(lapps_manager, body);
            async { result }
        })
        .await
}

fn update_lapp_handler(lapps_manager: &mut LappsManager, body: String) -> ServerResult<HttpResponse> {
    let request: LappUpdateRequest = serde_json::from_str(&body)?;
    let update_query = request.into_query();
    let lapp = lapps_manager.lapp_mut(&update_query.lapp_name)?;

    let updated = lapp.update(update_query)?;
    if updated.enabled.is_some() {
        let lapp_name = lapp.name().to_string();
        if lapp.enabled() {
            lapps_manager.load(lapp_name)?;
        } else {
            lapps_manager.unload(lapp_name);
        }
    }
    Ok(HttpResponse::Ok().json(LappResponse::Updated(updated)))
}
