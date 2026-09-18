#![allow(clippy::type_complexity, clippy::too_many_arguments)]

pub mod ai;
pub mod app;
pub mod autoplay;
pub mod deck;
pub mod elo;
pub mod engine;
pub mod help;
pub mod menu;
pub mod net;
pub mod os;
pub mod render;
pub mod settings;
pub mod table;
pub mod telemetry;
pub mod theme;
pub mod viewport;

pub use render::{FoilExtension, FoilMaterial};
pub use table::{CardTablePlugin, Tuning};
