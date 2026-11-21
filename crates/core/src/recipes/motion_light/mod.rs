//! The `motion-light` recipe, the port of Sowel's `sowel-recipe-motion-light` 1.1.2 with the same
//! id, slots, messages and behaviour (spec 012).

mod info;
mod machine;
mod params;
mod sources;

pub use info::{ID, info};
pub use machine::MotionLight;
pub use params::{MAX_BUTTONS, MAX_LIGHTS, Params, validate};
pub use sources::{MotionSource, is_motion_active, resolve_motion_sources};
