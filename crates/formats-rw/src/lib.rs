//! Readers for GTA Vice City (RenderWare, PC) file formats: IMG/DIR, DFF,
//! TXD, IFP and SFX.
//!
//! This crate only parses bytes. It never ships or embeds game data: every
//! file is read from the player's own installation. Each format is described
//! in `docs/formats/`, with the fields that were checked on real files.

pub mod dff;
pub mod ifp;
pub mod img;
pub mod rw;
pub mod sfx;
pub mod txd;

mod text;
