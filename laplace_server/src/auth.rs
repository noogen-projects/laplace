use std::fs;
use std::io::{BufReader, Write};
use std::path::Path;

use actix_web::dev::ServiceResponse;
use actix_web::error::Error;
use rcgen::{Certificate, CertificateParams, DistinguishedName, DnType};
use ring::rand;
use rustls::PrivateKey;
use rustls_pemfile::{certs, pkcs8_private_keys};

use crate::error::{AppError, AppResult};

pub mod middleware;

pub type AccessServiceResult = Result<ServiceResponse, Error>;

pub fn prepare_access_token(maybe_access_token: Option<String>) -> AppResult<&'static str> {
    let access_token = if let Some(access_token) = maybe_access_token {
        access_token
    } else {
        generate_token()?
    };

    // todo: use `String::leak` when its stabilized
    Ok(Box::leak(access_token.into_boxed_str()))
}

pub fn generate_token() -> AppResult<String> {
    let buf: [u8; 32] = rand::generate(&rand::SystemRandom::new())
        .map_err(|_| AppError::TokenGenerationFail)?
        .expose();
    Ok(bs58::encode(&buf).into_string())
}

pub fn prepare_certificates(
    certificate_path: &Path,
    private_key_path: &Path,
    host: impl Into<String>,
) -> AppResult<(Vec<rustls::Certificate>, PrivateKey)> {
    if !certificate_path.exists() && !private_key_path.exists() {
        log::info!("Generate SSL certificate");
        let certificate = generate_self_signed_certificate(vec![host.into()])?;

        if let Some(parent) = private_key_path.parent() {
            fs::create_dir_all(parent)?;
        }
        if let Some(parent) = certificate_path.parent() {
            fs::create_dir_all(parent)?;
        }

        fs::File::create(private_key_path)?.write_all(certificate.serialize_private_key_pem().as_bytes())?;
        fs::File::create(certificate_path)?.write_all(certificate.serialize_pem()?.as_bytes())?;
    }

    log::info!("Bind SSL");
    let certificates = certs(&mut BufReader::new(fs::File::open(certificate_path)?))?
        .into_iter()
        .map(rustls::Certificate)
        .collect();

    let private_key = PrivateKey(
        pkcs8_private_keys(&mut BufReader::new(fs::File::open(private_key_path)?))?
            .into_iter()
            .next()
            .ok_or(AppError::MissingPrivateKey)?,
    );

    Ok((certificates, private_key))
}

pub fn generate_self_signed_certificate(subject_alt_names: impl Into<Vec<String>>) -> AppResult<Certificate> {
    let mut distinguished_name = DistinguishedName::new();
    distinguished_name.push(DnType::CommonName, "Laplace self signed cert");
    distinguished_name.push(DnType::OrganizationName, "Laplace community");

    let mut params = CertificateParams::new(subject_alt_names);
    params.distinguished_name = distinguished_name;

    Certificate::from_params(params).map_err(Into::into)
}
