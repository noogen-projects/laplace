use crate::daps::Dap;
use actix_web::{
    cookie::Cookie,
    dev::{AnyBody, ServiceRequest, ServiceResponse},
    http, HttpResponse,
};

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

        let dap_name = uri
            .path()
            .split('/')
            .skip_while(|chunk| chunk.is_empty())
            .next()
            .unwrap_or(Dap::main_name());

        let access_token_cookie = Cookie::build("access_token", access_token)
            .domain(uri.host().unwrap_or(""))
            .path(format!("/{}", dap_name))
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
