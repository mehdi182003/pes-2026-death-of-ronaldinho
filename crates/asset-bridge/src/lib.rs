//! Bridge between the format crates and the game.
//!
//! Converts parsed Vice City and PES 6 files into engine-neutral types, and
//! owns the configuration of the game install paths and the extraction cache.
//! The `game` crate only ever talks to this crate, never to the format crates.

pub mod config;
