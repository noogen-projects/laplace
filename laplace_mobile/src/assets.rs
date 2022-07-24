use std::{
    error::Error,
    ffi::{CStr, CString},
    fs, io,
    path::{Path, PathBuf},
};

use jni::{objects::JObject, JNIEnv, JavaVM};
use ndk::asset::Asset;

pub type CopyResult<T> = Result<T, Box<dyn Error>>;

pub fn copy(asset_dirs: impl IntoIterator<Item = impl AsRef<Path>>, destination: impl AsRef<Path>) -> CopyResult<()> {
    // Create a VM for executing Java calls
    let ctx = ndk_context::android_context();
    let vm = unsafe { JavaVM::from_raw(ctx.vm().cast()) }?;
    let env = vm.attach_current_thread()?;

    // Query the Asset Manager
    let asset_manager = env
        .call_method(
            ctx.context().cast(),
            "getAssets",
            "()Landroid/content/res/AssetManager;",
            &[],
        )?
        .l()?;

    // Copy assets
    for asset_dir in asset_dirs {
        copy_recursively(
            *env,
            asset_manager,
            asset_dir.as_ref().to_path_buf(),
            destination.as_ref().join(asset_dir),
        )?;
    }

    Ok(())
}

fn copy_recursively(
    env: JNIEnv,
    asset_manager: JObject,
    asset_dir: PathBuf,
    destination_dir: PathBuf,
) -> CopyResult<()> {
    for asset_filename in list(env, asset_manager, &asset_dir)? {
        let asset_path = asset_dir.join(&asset_filename);
        if let Some(asset) = open_asset(&CString::new(asset_path.to_string_lossy().as_ref())?) {
            copy_asset(asset, asset_filename, &destination_dir)?;
        } else {
            copy_recursively(env, asset_manager, asset_path, destination_dir.join(asset_filename))?;
        }
    }
    Ok(())
}

fn list(env: JNIEnv, asset_manager: JObject, asset_dir: &Path) -> CopyResult<Vec<String>> {
    let asset_array = env
        .call_method(asset_manager, "list", "(Ljava/lang/String;)[Ljava/lang/String;", &[env
            .new_string(asset_dir.to_string_lossy())?
            .into()])?
        .l()?
        .into_inner();

    let mut assets = Vec::new();
    for index in 0..env.get_array_length(asset_array)? {
        let asset: String = env
            .get_string(env.get_object_array_element(asset_array, index)?.into())?
            .into();
        assets.push(asset);
    }

    Ok(assets)
}

fn open_asset(asset_path: &CStr) -> Option<Asset> {
    #[allow(deprecated)]
    ndk_glue::native_activity().asset_manager().open(asset_path)
}

fn copy_asset(mut asset: Asset, filename: impl AsRef<Path>, destination_dir: impl AsRef<Path>) -> CopyResult<()> {
    fs::create_dir_all(destination_dir.as_ref())?;
    let mut file = fs::File::options()
        .create(true)
        .write(true)
        .open(destination_dir.as_ref().join(filename))?;

    io::copy(&mut asset, &mut file)?;
    Ok(())
}
