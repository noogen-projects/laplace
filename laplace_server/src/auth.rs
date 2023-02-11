use actix_web::dev::ServiceResponse;
use actix_web::error::Error;
use rcgen::{Certificate, CertificateParams, DistinguishedName, DnType};
use ring::rand;

use crate::error::{AppError, AppResult};

pub mod middleware;

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
