//! Shared decisions only: platform adapters own capture, input and positive observations.
pub mod clock;
pub mod engine;
pub mod model;
mod seat_selection;
pub mod nol_captcha;
pub mod vision;
pub mod selection_count;
mod selection_count_layout;
pub mod seat_objects;
pub mod visual_dialog;

pub use engine::Engine;
pub use model::*;
