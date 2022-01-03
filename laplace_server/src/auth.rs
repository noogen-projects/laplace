use actix_web::{
    cookie::Cookie,
    dev::{AnyBody, Service, ServiceRequest, ServiceResponse},
    error::Error,
    http, HttpResponse,
};
use futures::future::{self, Either, Ready};

use crate::{
    error::error_response,
    lapps::{Lapp, LappsProvider},
};

pub type AccessServiceResult = Result<ServiceResponse<AnyBody>, Error>;

pub fn create_check_access_middleware<S>(
    lapps_provider: LappsProvider,
    laplace_access_token: impl Into<String>,
) -> impl Fn(ServiceRequest, &S) -> Either<S::Future, Ready<AccessServiceResult>> + Clone
where
    S: Service<ServiceRequest, Response = ServiceResponse<AnyBody>, Error = Error>,
{
    let laplace_access_token = laplace_access_token.into();

    move |request, service: &S| {
        let request = match query_access_token_redirect(request) {
            Ok(response) => return Either::Right(future::ok(response)),
            Err(request) => request,
        };

        let lapp_name = request
            .path()
            .split('/')
            .skip_while(|chunk| chunk.is_empty())
            .next()
            .unwrap_or_default();

        let access_token = request
            .cookie("access_token")
            .map(|cookie| cookie.value().to_string())
            .unwrap_or_default();

        if lapp_name.is_empty() || lapp_name == "static" {
            return Either::Left(service.call(request));
        }

        if lapp_name == Lapp::main_name() {
            if access_token == laplace_access_token.as_str() {
                Either::Left(service.call(request))
            } else {
                Either::Right(future::ok(request.into_response(HttpResponse::Forbidden().finish())))
            }
        } else {
            match lapps_provider.lapp(lapp_name).map(|lapp| {
                access_token.as_str() == lapp.settings().application.access_token.as_deref().unwrap_or_default()
            }) {
                Ok(true) => Either::Left(service.call(request)),
                Ok(false) => {
                    let response = request.into_response(HttpResponse::Forbidden().finish());
                    Either::Right(future::ok(response))
                },
                Err(err) => Either::Right(future::ok(request.into_response(error_response(err)))),
            }
        }
    }
}

pub fn query_access_token_redirect(request: ServiceRequest) -> Result<ServiceResponse<AnyBody>, ServiceRequest> {
    let uri = request.uri().clone();
    let query = uri.query().unwrap_or_default();

    if query.starts_with("access_token=") || query.contains("&access_token=") {
        let mut access_token = "";
        let mut new_query = String::new();

        for param in query.split('&') {
            let pair: Vec<_> = param.split('=').collect();
            if pair[0] == "access_token" {
                access_token = pair[1];
            } else {
                new_query.push(if new_query.is_empty() { '?' } else { '&' });
                new_query.push_str(param)
            }
        }

        let lapp_name = uri
            .path()
            .split('/')
            .skip_while(|chunk| chunk.is_empty())
            .next()
            .unwrap_or(Lapp::main_name());

        let access_token_cookie = Cookie::build("access_token", access_token)
            .domain(uri.host().unwrap_or(""))
            .path(format!("/{}", lapp_name))
            .secure(true)
            .http_only(true)
            .finish();

        let response = request.into_response(
            HttpResponse::Found()
                .append_header((http::header::LOCATION, format!("{}{}", uri.path(), new_query)))
                .cookie(access_token_cookie)
                .finish(),
        );
        Ok(response)
    } else {
        Err(request)
    }
}
