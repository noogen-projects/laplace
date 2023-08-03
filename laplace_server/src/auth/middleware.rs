use std::rc::Rc;

use actix_web::cookie::time::Duration;
use actix_web::cookie::Cookie;
use actix_web::dev::{forward_ready, Service, ServiceRequest, ServiceResponse, Transform};
use actix_web::error::Error;
use actix_web::{http, web, HttpResponse};
use futures::future::{self, LocalBoxFuture};

use crate::error::error_response;
use crate::lapps::{Lapp, LappsProvider};

pub struct CheckAccess {
    lapps_provider: web::Data<LappsProvider>,
    laplace_access_token: &'static str,
}

impl CheckAccess {
    pub fn new(lapps_provider: web::Data<LappsProvider>, laplace_access_token: &'static str) -> Self {
        Self {
            lapps_provider,
            laplace_access_token,
        }
    }
}

impl<S: 'static> Transform<S, ServiceRequest> for CheckAccess
where
    S: Service<ServiceRequest, Response = ServiceResponse, Error = Error>,
    S::Future: 'static,
{
    type Response = ServiceResponse;
    type Error = Error;
    type Transform = CheckAccessMiddleware<S>;
    type InitError = ();
    type Future = future::Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        future::ready(Ok(CheckAccessMiddleware {
            service: Rc::new(service),
            lapps_provider: self.lapps_provider.clone(),
            laplace_access_token: self.laplace_access_token,
        }))
    }
}

pub struct CheckAccessMiddleware<S> {
    service: Rc<S>,
    lapps_provider: web::Data<LappsProvider>,
    laplace_access_token: &'static str,
}

impl<S> Service<ServiceRequest> for CheckAccessMiddleware<S>
where
    S: Service<ServiceRequest, Response = ServiceResponse, Error = Error> + 'static,
    S::Future: 'static,
{
    type Response = ServiceResponse;
    type Error = Error;
    type Future = LocalBoxFuture<'static, Result<Self::Response, Self::Error>>;

    forward_ready!(service);

    fn call(&self, request: ServiceRequest) -> Self::Future {
        let service = self.service.clone();
        let lapps_provider = self.lapps_provider.clone();
        let laplace_access_token = self.laplace_access_token;

        Box::pin(async move {
            let request = match query_access_token_redirect(request) {
                Ok(response) => return Ok(response),
                Err(request) => request,
            };

            let lapp_name = request
                .path()
                .split('/')
                .find(|chunk| !chunk.is_empty())
                .unwrap_or_default()
                .to_string();

            if lapp_name.is_empty() || lapp_name == "static" {
                return service.call(request).await;
            }

            let access_token = request
                .cookie("access_token")
                .map(|cookie| cookie.value().to_string())
                .unwrap_or_default();

            if lapp_name == Lapp::main_name() {
                if access_token == laplace_access_token {
                    service.call(request).await
                } else {
                    Ok(request.into_response(HttpResponse::Forbidden().finish()))
                }
            } else {
                match lapps_provider.read_manager().await.lapp(&lapp_name) {
                    Ok(lapp) => {
                        if access_token
                            == lapp
                                .read()
                                .await
                                .settings()
                                .application
                                .access_token
                                .as_deref()
                                .unwrap_or_default()
                        {
                            service.call(request).await
                        } else {
                            log::warn!(
                                "Access denied for lapp \"{}\" with access token \"{}\"",
                                lapp_name,
                                access_token
                            );
                            Ok(request.into_response(HttpResponse::Forbidden().finish()))
                        }
                    },
                    Err(err) => Ok(request.into_response(error_response(err))),
                }
            }
        })
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
            .find(|chunk| !chunk.is_empty())
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
