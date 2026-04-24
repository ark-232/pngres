//! Frontend assets baked into the binary via rust-embed.

use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "../../web/dist"]
pub struct Assets;
