//! # engine
//!
//! 3D Movie Maker domain model: Movie, Scene, Actor, events, routes.
//!
//! Module hierarchy (bottom-up dependency order):
//!   fixedpoint → transform → tag → events → actor → scene → movie

pub mod actor;
pub mod background;
pub mod error;
pub mod events;
pub mod fixedpoint;
pub mod material;
pub mod model;
pub mod movie;
pub mod msnd;
pub mod scene;
pub mod tag;
pub mod tdf;
pub mod tdt;
pub mod tmap;
pub mod transform;
