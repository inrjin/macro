//! Shared decisions only: platform adapters own capture, input and positive observations.
pub mod clock;
pub mod engine;
pub mod model;
mod seat_selection;
pub mod nol_captcha;
pub mod vision;
pub mod visual_dialog;

pub use engine::Engine;
pub use model::*;
