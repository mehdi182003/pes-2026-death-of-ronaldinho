//! Readers for PES 6 (PC) file formats: AFS archives, internal compression,
//! models and animations.
//!
//! This crate only parses bytes. It never ships or embeds game data: every
//! file is read from the player's own installation.
//!
//! Each format is described in `docs/formats/`, with the fields that were
//! checked on real files.

pub mod afs;
pub mod packed;
