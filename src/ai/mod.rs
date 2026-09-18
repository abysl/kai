pub mod brain;
pub mod cards;
pub mod decks;
pub mod driver;
pub mod hold;
#[cfg(not(target_arch = "wasm32"))]
pub mod local;
#[cfg(target_arch = "wasm32")]
pub use web_local as local;
pub mod nanogpt;
pub mod provider;
pub mod random;
pub mod seat;
pub mod setup;
pub mod soak;
#[cfg(target_arch = "wasm32")]
pub mod web_http;
#[cfg(any(test, target_arch = "wasm32"))]
pub mod web_local;
