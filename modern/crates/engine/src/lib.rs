//! # engine
//!
//! 3D Movie Maker domain model: Movie, Scene, Actor, events, routes.
//!
//! Module hierarchy (bottom-up dependency order):
//!   fixedpoint → transform → tag → events → actor → scene → movie

pub mod error;
pub mod fixedpoint;
pub mod tag;
pub mod transform;
pub mod events;
pub mod model;
pub mod material;
pub mod tmap;
pub mod background;
pub mod actor;
pub mod scene;
pub mod movie;
