use std::io;

pub use actix_files;
pub use actix_web;

use actix_files::{Files, NamedFile};
use actix_web::{dev::Service, http, middleware, web, App, HttpResponse, HttpServer};
use futures::future;
use log::error;

use self::{
    daps::{Dap, DapsProvider},
    error::{error_response, ServerError},
    settings::Settings,
};

pub mod auth;
pub mod convert;
pub mod daps;
pub mod error;
pub mod gossipsub;
pub mod handler;
pub mod settings;
pub mod ws;

pub async fn run(settings: Settings) -> io::Result<()> {
    let daps_path = settings.daps.path.clone();
    let daps_provider = web::block(move || DapsProvider::new(daps_path))
        .await
        .expect("Daps provider should be constructed")?;
    let web_root = settings.http.web_root.clone();
    let dapla_access_token = settings.http.access_token.clone().unwrap_or_default();

    HttpServer::new(move || {
        let static_dir = web_root.join(Dap::static_dir_name());
        let dapla_uri = format!("/{}", Dap::main_name());

        let mut app = App::new()
            .app_data(web::Data::new(daps_provider.clone()))
            .wrap(middleware::DefaultHeaders::new().header("X-Version", "0.2"))
            .wrap(middleware::NormalizePath::trim())
            .wrap_fn({
                let daps_provider = daps_provider.clone();
                let dapla_access_token = dapla_access_token.clone();
                move |request, service| {
                    let request = match auth::query_access_token_redirect(request) {
                        Ok(response) => return future::Either::Right(future::ok(response)),
                        Err(request) => request,
                    };

                    let dap_name = request
                        .path()
                        .split('/')
                        .skip_while(|chunk| chunk.is_empty())
                        .next()
                        .unwrap_or_default();

                    let access_token = request
                        .cookie("access_token")
                        .map(|cookie| cookie.value().to_string())
                        .unwrap_or_default();

                    if dap_name.is_empty()
                        || dap_name == "static"
                        || (dap_name == Dap::main_name() && access_token == dapla_access_token.as_str())
                    {
                        return future::Either::Left(service.call(request));
                    }

                    let daps_manager = match daps_provider.lock() {
                        Ok(daps_manager) => daps_manager,
                        Err(err) => {
                            error!("Daps service lock should be asquired: {:?}", err);
                            return future::Either::Right(future::ok(
                                request.into_response(error_response(ServerError::DapsServiceNotLock)),
                            ));
                        },
                    };

                    match daps_manager.dap(dap_name) {
                        Ok(dap) => {
                            if access_token.as_str()
                                == dap.settings().application.access_token.as_deref().unwrap_or_default()
                            {
                                future::Either::Left(service.call(request))
                            } else {
                                let response = request.into_response(HttpResponse::Forbidden().finish());
                                future::Either::Right(future::ok(response))
                            }
                        },
                        Err(err) => future::Either::Right(future::ok(request.into_response(error_response(err)))),
                    }
                }
            })
            .wrap(middleware::Compress::default())
            .wrap(middleware::Logger::default())
            .service(Files::new(&Dap::main_static_uri(), &static_dir).index_file(Dap::index_file_name()))
            .route(
                "/",
                web::route().to({
                    let dapla_uri = dapla_uri.clone();
                    move || {
                        HttpResponse::Found()
                            .append_header((http::header::LOCATION, dapla_uri.as_str()))
                            .finish()
                    }
                }),
            )
            .route(
                &dapla_uri,
                web::get().to(move || {
                    let index_file = static_dir.join(Dap::index_file_name());
                    async { NamedFile::open(index_file) }
                }),
            )
            .route(&Dap::main_uri("daps"), web::get().to(handler::get_daps))
            .route(&Dap::main_uri("dap"), web::post().to(handler::update_dap));

        let mut daps_manager = daps_provider.lock().expect("Daps manager lock should be acquired");
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
