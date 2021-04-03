use std::io;

pub use actix_files;
use actix_files::{Files, NamedFile};
pub use actix_web;
use actix_web::{middleware, web, App, HttpServer};

use self::{
    daps::{Dap, DapsService},
    settings::Settings,
};

pub mod daps;
pub mod error;
pub mod settings;
pub mod handler {
    use std::{borrow::Cow, ops::Deref};

    use actix_web::{web, HttpResponse};

    use crate::daps::{DapResponse, DapUpdateRequest, DapsService};

    pub async fn get_daps(daps_service: web::Data<DapsService>) -> HttpResponse {
        daps_service
            .into_inner()
            .handle_http(|daps_manager| {
                let mut daps: Vec<_> = daps_manager
                    .daps_iter()
                    .filter(|dap| !dap.is_main())
                    .map(|dap| Cow::Borrowed(dap.deref()))
                    .collect();
                daps.sort_unstable_by(|a, b| a.name().cmp(b.name()));

                Ok(HttpResponse::Ok().json(DapResponse::Daps(daps)))
            })
            .await
    }

    pub async fn update_dap(daps_service: web::Data<DapsService>, body: String) -> HttpResponse {
        daps_service
            .into_inner()
            .handle_http(|daps_manager| {
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
            })
            .await
    }
}

pub async fn run(settings: Settings) -> io::Result<()> {
    let daps_service = DapsService::new(&settings.daps.path)?;
    let web_root = settings.http.web_root.clone();

    HttpServer::new(move || {
        let static_dir = web_root.join(Dap::static_dir_name());

        let mut app = App::new()
            .data(daps_service.clone())
            .wrap(middleware::DefaultHeaders::new().header("X-Version", "0.2"))
            .wrap(middleware::NormalizePath::default())
            .wrap(middleware::Compress::default())
            .wrap(middleware::Logger::default())
            .service(Files::new(&Dap::main_static_uri(), &static_dir).index_file(Dap::index_file_name()))
            .route(
                "/",
                web::get().to(move || {
                    let index_file = static_dir.join(Dap::index_file_name());
                    async { NamedFile::open(index_file) }
                }),
            )
            .route(&Dap::main_uri("daps"), web::get().to(handler::get_daps))
            .route(&Dap::main_uri("dap"), web::post().to(handler::update_dap));

        let mut daps_manager = daps_service.lock().expect("Daps manager lock should be acquired");
        daps_manager.load_daps();

        for dap in daps_manager.daps_iter() {
            app = app.configure(dap.http_configure());
        }
        app
    })
    .bind((settings.http.host.as_str(), settings.http.port))?
    .run()
    .await
}
