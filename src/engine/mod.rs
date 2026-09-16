pub mod modules;
#[cfg(not(target_arch = "wasm32"))]
mod native;
pub mod selection;
#[cfg(not(target_arch = "wasm32"))]
pub use native::*;
#[cfg(target_arch = "wasm32")]
pub mod web;
#[cfg(target_arch = "wasm32")]
pub use web::*;
