#[cfg(target_os = "android")]
pub mod android;
pub mod clipboard;
pub mod drafts;
pub mod entropy;
#[cfg(all(not(target_os = "android"), not(target_arch = "wasm32")))]
pub mod icon;
pub mod ime;
pub mod paths;
pub mod profile;
pub mod qr;
