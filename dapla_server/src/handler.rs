use std::{borrow::Cow, ops::Deref};

use actix_web::{web, HttpResponse};

use crate::{
    daps::{DapResponse, DapUpdateRequest, DapsManager, DapsProvider},
    error::ServerResult,
};

pub async fn get_daps(daps_service: web::Data<DapsProvider>) -> HttpResponse {
    daps_service
        .into_inner()
        .handle(|daps_manager| {
            let mut daps: Vec<_> = daps_manager
                .daps_iter()
                .filter(|dap| !dap.is_main())
                .map(|dap| Cow::Borrowed(dap.deref()))
                .collect();
            daps.sort_unstable_by(|a, b| a.name().cmp(b.name()));
            let response = HttpResponse::Ok().json(DapResponse::Daps(daps));

            async { Ok(response) }
        })
        .await
}

pub async fn update_dap(daps_service: web::Data<DapsProvider>, body: String) -> HttpResponse {
    daps_service
        .into_inner()
        .handle(|daps_manager| {
            let result = update_dap_handler(daps_manager, body);
            async { result }
        })
        .await
}

fn update_dap_handler(daps_manager: &mut DapsManager, body: String) -> ServerResult<HttpResponse> {
    let request: DapUpdateRequest = serde_json::from_str(&body)?;
    let update_query = request.into_query();
    let dap = daps_manager.dap_mut(&update_query.dap_name)?;

    let updated = dap.update(update_query)?;
    if updated.enabled.is_some() {
        let dap_name = dap.name().to_string();
        if dap.enabled() {
            daps_manager.load(dap_name)?;
        } else {
            daps_manager.unload(dap_name);
        }
    }
    Ok(HttpResponse::Ok().json(DapResponse::Updated(updated)))
}
