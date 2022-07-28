use actix_web::{
    cookie::{time::Duration, Cookie},
    dev::{Service, ServiceRequest, ServiceResponse},
    error::Error,
    http, web, HttpResponse,
};
use futures::{
    future::{self, Either, Ready},
    FutureExt,
};
use rcgen::{Certificate, CertificateParams, DistinguishedName, DnType};
use ring::rand;

use crate::{
    error::{error_response, AppError, AppResult},
    lapps::{Lapp, LappsProvider},
};

pub type AccessServiceResult = Result<ServiceResponse, Error>;

pub fn generate_token() -> AppResult<String> {
    let buf: [u8; 32] = rand::generate(&rand::SystemRandom::new())
        .map_err(|_| AppError::TokenGenerationFail)?
        .expose();
    Ok(bs58::encode(&buf).into_string())
}

pub fn generate_self_signed_certificate(subject_alt_names: impl Into<Vec<String>>) -> AppResult<Certificate> {
    let mut distinguished_name = DistinguishedName::new();
    distinguished_name.push(DnType::CommonName, "Laplace self signed cert");
    distinguished_name.push(DnType::OrganizationName, "Laplace community");

    let mut params = CertificateParams::new(subject_alt_names);
    params.distinguished_name = distinguished_name;

    Certificate::from_params(params).map_err(Into::into)
}

pub fn create_check_access_middleware<S>(
    lapps_provider: web::Data<LappsProvider>,
    laplace_access_token: impl Into<String>,
) -> impl Fn(ServiceRequest, &S) -> Either<S::Future, Ready<AccessServiceResult>> + Clone
where
    S: Service<ServiceRequest, Response = ServiceResponse, Error = Error>,
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
            let is_access_allowed = lapps_provider
                .read_manager()
                .and_then(|manager| {
                    manager.lapp(lapp_name).map(|lapp| {
                        access_token.as_str() == lapp.settings().application.access_token.as_deref().unwrap_or_default()
                    })
                })
                .map_err(error_response);

            match is_access_allowed {
                Ok(true) => service.call(request).left_future(),
                Ok(false) => {
                    log::warn!(
                        "Access denied for lapp \"{}\" with access token \"{}\"",
                        lapp_name,
                        access_token
                    );
                    let response = request.into_response(HttpResponse::Forbidden().finish());
                    future::ok(response).right_future()
                },
                Err(err) => future::ok(request.into_response(error_response(err))).right_future(),
            }
        }
    }
}

pub fn query_access_token_redirect(request: ServiceRequest) -> Result<ServiceResponse, ServiceRequest> {
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
            .http_only(true)
            .max_age(Duration::days(365 * 10)) // 10 years
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
