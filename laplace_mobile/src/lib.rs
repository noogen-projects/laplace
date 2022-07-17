use actix_web::rt::System;
use flexi_logger::{Duplicate, FileSpec, Logger};
use laplace_server::settings::Settings;
use log::info;

mod panic;

#[cfg_attr(target_os = "android", ndk_glue::main(backtrace = "on"))]
pub fn main() {
    Logger::try_with_env_or_str("info,regalloc=warn,wasmer_compiler_cranelift=warn,cranelift_codegen=warn")
        .unwrap()
        .log_to_file(
            FileSpec::default()
                .directory("/sdcard/Android/data/rust.laplace_mobile/files")
                .basename("laplace")
                .suppress_timestamp()
                .suffix("log"),
        )
        .duplicate_to_stdout(Duplicate::All)
        .start()
        .unwrap();

    panic::set_logger_hook();

    info!(
        "Internal data path: {}",
        ndk_glue::native_activity().internal_data_path().to_str().unwrap()
    );
    info!(
        "External data path: {}",
        ndk_glue::native_activity().external_data_path().to_str().unwrap()
    );
    info!("Load settings file");
    let settings = Settings::new("/sdcard/Android/data/rust.laplace_mobile/files/settings.toml")
        .expect("Settings should be configured");

    info!("Create actix system");
    System::new()
        .block_on(async move { laplace_server::run(settings).await })
        .expect("Laplace run error")
}
