use std::{fs, path::PathBuf};

use actix_web::rt::System;
use laplace_server::{auth::generate_token, settings::Settings};
use log::info;

mod assets;
mod panic;

fn get_data_path() -> &'static str {
    #[allow(deprecated)]
    ndk_glue::native_activity()
        .external_data_path()
        .to_str()
        .expect("Wrong external data path")
}

#[cfg_attr(target_os = "android", ndk_glue::main(backtrace = "on"))]
pub fn main() {
    let data_path = PathBuf::from(get_data_path());
    let settings_path = data_path.join("settings.toml");
    let settings = if let Ok(settings) = Settings::new(&settings_path) {
        settings
    } else {
        let mut settings = Settings::default();
        settings.http.web_root = data_path.join("web_root");
        settings.http.access_token = Some(generate_token());
        settings.lapps.path = settings.http.web_root.join("lapps");
        settings.log.dir = Some(data_path.join("log"));
        settings.log.spec = "info,regalloc=warn,wasmer_compiler_cranelift=warn,cranelift_codegen=warn".into();
        settings.ssl.enabled = false;
        settings.ssl.private_key_path = data_path.join("cert").join("key.pem");
        settings.ssl.certificate_path = data_path.join("cert").join("cert.pem");

        let serialized_settings = toml::to_string(&settings).expect("Cannot serialize settings");
        fs::write(settings_path, serialized_settings).expect("Cannot write settings");

        settings
    };

    laplace_server::init_logger(&settings.log);
    panic::set_logger_hook();

    if !settings.lapps.path.exists()
        || (settings.lapps.path.is_dir()
            && settings
                .lapps
                .path
                .read_dir()
                .map(|mut dir| dir.next().is_none())
                .unwrap_or(false))
    {
        info!("Copy assets");
        assets::copy(["lapps", "static"], &settings.http.web_root).expect("Copy assets error");
    }

    info!("Create actix system");
    System::new()
        .block_on(async move { laplace_server::run(settings).await })
        .expect("Laplace run error")
}
