//! The App logos of the Home Tiles: official artwork, never recoloured (ADR 0005), so they are PNGs
//! with a file per theme in `design/logos`, compiled in. An App without a file shows its name.

use std::{collections::HashMap, sync::Arc};

use gpui::RenderImage;
use m4tv_shell::shell::Theme;
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "../../design/logos"]
#[include = "*.png"]
struct Files;

/// Decoded logos, kept for as long as the Shell runs: a handful of small images.
#[derive(Default)]
pub struct Logos(HashMap<String, Option<Arc<RenderImage>>>);

impl Logos {
    pub fn get(&mut self, app: &str, theme: Theme) -> Option<Arc<RenderImage>> {
        let name = format!("{app}-{}.png", if theme == Theme::Dark { "dark" } else { "light" });
        self.0.entry(name.clone()).or_insert_with(|| decode(&name)).clone()
    }
}

fn decode(name: &str) -> Option<Arc<RenderImage>> {
    let file = Files::get(name)?;
    let mut pixels = image::load_from_memory(&file.data).ok()?.to_rgba8();
    // gpui draws premultiplied BGRA; a PNG is straight RGBA.
    for pixel in pixels.chunks_exact_mut(4) {
        let alpha = u16::from(pixel[3]);
        for channel in &mut pixel[..3] {
            *channel = (u16::from(*channel) * alpha / 255) as u8;
        }
        pixel.swap(0, 2);
    }
    Some(Arc::new(RenderImage::new(vec![image::Frame::new(pixels)])))
}
