pub mod laplace {
    use std::path::PathBuf;

    use actix_files::NamedFile;
    use actix_web::{web, Scope};

    use crate::lapps::Lapp;

    pub mod handler;

    pub fn services(
        laplace_uri: &str,
        static_dir: impl Into<PathBuf>,
        lapps_dir: impl Into<PathBuf>,
        route_file_path: &str,
    ) -> Scope {
        let static_dir = static_dir.into();
        let lapps_dir = lapps_dir.into();

        web::scope(laplace_uri)
            .service(web::resource(["", "/"]).route(web::get().to(move || {
                let index_file = static_dir.join(Lapp::index_file_name());
                async { NamedFile::open(index_file) }
            })))
            .route(
                route_file_path,
                web::get().to(move |file_path: web::Path<String>, request| {
                    let file_path = lapps_dir
                        .join(Lapp::main_name())
                        .join(Lapp::static_dir_name())
                        .join(&*file_path);

                    async move { NamedFile::open(file_path).map(|file| file.into_response(&request)) }
                }),
            )
            .route("/lapps", web::get().to(handler::get_lapps))
            .route("/lapp/add", web::post().to(handler::add_lapp))
            .route("/lapp/update", web::post().to(handler::update_lapp))
    }
}

pub mod lapp {
    use actix_web::{web, Scope};

    pub mod handler;

    pub fn services(route_file_path: &str) -> Scope {
        web::scope("/{lapp_name}")
            .service(web::resource(["", "/"]).route(
                web::get().to(move |lapps_service, lapp_name, request| {
                    handler::index_file(lapps_service, lapp_name, request)
                }),
            ))
            .route(
                route_file_path,
                web::get().to({
                    move |lapps_service, path: web::Path<(String, String)>, request| {
                        let (lapp_name, file_path) = path.into_inner();
                        handler::static_file(lapps_service, lapp_name, file_path, request)
                    }
                }),
            )
            .route(
                "/ws",
                web::get().to(move |lapps_service, lapp_name, request, stream| {
                    handler::ws_start(lapps_service, lapp_name, request, stream)
                }),
            )
            .route(
                "/p2p",
                web::post().to(move |lapps_service, lapp_name, request| {
                    handler::gossipsub_start(lapps_service, lapp_name, request)
                }),
            )
            .route(
                "/{tail}*",
                web::route().to(move |lapps_service, path: web::Path<(String, String)>, request, body| {
                    let (lapp_name, _tail) = path.into_inner();
                    handler::http(lapps_service, lapp_name, request, body)
                }),
            )
    }
}
