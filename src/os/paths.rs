use std::path::PathBuf;

#[cfg(all(not(target_os = "android"), not(target_arch = "wasm32")))]
pub fn store_dir() -> Option<PathBuf> {
    Some(
        std::env::var("SPIRIT_STORE")
            .map(PathBuf::from)
            .unwrap_or_else(|_| config_home().join(".spirit/store")),
    )
}

#[cfg(target_os = "android")]
pub fn store_dir() -> Option<PathBuf> {
    crate::os::android::store_dir()
}

#[cfg(target_arch = "wasm32")]
pub fn store_dir() -> Option<PathBuf> {
    None
}

#[cfg(all(not(target_os = "android"), not(target_arch = "wasm32")))]
fn config_home() -> PathBuf {
    PathBuf::from(std::env::var("HOME").unwrap_or_default())
}

#[cfg(all(not(target_os = "android"), not(target_arch = "wasm32")))]
pub fn config_dir() -> PathBuf {
    std::env::var("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| config_home().join(".config"))
        .join("kai")
}

#[cfg(target_os = "android")]
pub fn config_dir() -> PathBuf {
    crate::os::android::config_dir()
}
