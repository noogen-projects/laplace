use axum::extract::State;
use axum::http::{header, Request, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Redirect, Response};
use cookie::time::Duration;
use cookie::Cookie;

use crate::lapps::{Lapp, LappsProvider};
use crate::web_api::{err_into_json_response, ResultResponse};

pub async fn check_access<B>(
    State((lapps_provider, laplace_access_token)): State<(LappsProvider, &'static str)>,
    request: Request<B>,
    next: Next<B>,
) -> ResultResponse<Response> {
    let request = match query_access_token_redirect(request) {
        Ok(response) => return Ok(response),
        Err(request) => request,
    };

    let lapp_name = request
        .uri()
        .path()
        .split('/')
        .find(|chunk| !chunk.is_empty())
        .unwrap_or_default()
        .to_string();

    if lapp_name.is_empty() || lapp_name == "static" || lapp_name == "favicon.ico" {
        Ok(next.run(request).await)
    } else {
        let access_token = request
            .headers()
            .get_all(header::COOKIE)
            .into_iter()
            .filter_map(|cookie_value| Cookie::parse(cookie_value.to_str().ok()?).ok())
            .find(|cookie| cookie.name() == "access_token")
            .map(|cookie| cookie.value().to_string())
            .unwrap_or_default();

        if lapp_name == Lapp::main_name() {
            if access_token == laplace_access_token {
                Ok(next.run(request).await)
            } else {
                let mut response = Response::default();
                *response.status_mut() = StatusCode::FORBIDDEN;
                Ok(response)
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
                        Ok(next.run(request).await)
                    } else {
                        log::warn!(
                            "Access denied for lapp \"{}\" with access token \"{}\"",
                            lapp_name,
                            access_token
                        );

                        let mut response = Response::default();
                        *response.status_mut() = StatusCode::FORBIDDEN;
                        Ok(response)
                    }
                },
                Err(err) => Err(err_into_json_response(err)),
            }
        }
    }
}

pub fn query_access_token_redirect<B>(request: Request<B>) -> Result<Response, Request<B>> {
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

        let mut response = Redirect::to(&format!("{}{}", uri.path(), new_query)).into_response();
        response.headers_mut().insert(
            header::SET_COOKIE,
            access_token_cookie.to_string().try_into().map_err(|_| request)?,
        );

        Ok(response)
    } else {
        Err(request)
    }
}
