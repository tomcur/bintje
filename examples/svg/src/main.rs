//! A Bintje example rendering an SVG.

use std::path::Path;

use image::ImageEncoder;
use kurbo::Affine;
use peniko::color::PremulRgba8;
use pico_svg::Item;

use bintje::{cpu_rasterize, Bintje};

pub mod pico_svg;

/// Render an SVG.
pub fn main() {
    let scale = 1.;
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../assets/tiger/Ghostscript_Tiger.svg");
    // let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../assets/potato/1f954.svg");
    let svg = pico_svg::PicoSvg::load(&std::fs::read_to_string(&path).unwrap(), scale).unwrap();

    #[expect(
        clippy::cast_possible_truncation,
        reason = "truncate to max pixel dimensions"
    )]
    let mut renderer = Bintje::new(
        (svg.size.width * scale).ceil() as u16,
        (svg.size.height * scale).ceil() as u16,
    );

    encode_svg(&mut renderer, Affine::IDENTITY, &svg.items);

    let commands = renderer.commands();
    let (width, height) = renderer.size();
    let mut img = vec![PremulRgba8::from_u32(0); width as usize * height as usize];
    cpu_rasterize(
        width,
        height,
        &mut img,
        commands.alpha_masks,
        commands.wide_tiles,
    );

    let file = std::fs::OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open("test.png")
        .unwrap();
    let encoder = image::codecs::png::PngEncoder::new(file);
    encoder
        .write_image(
            bytemuck::cast_slice(&img),
            width as u32,
            height as u32,
            image::ExtendedColorType::Rgba8,
        )
        .unwrap();
}

fn encode_svg(renderer: &mut Bintje, transform: Affine, items: &[Item]) {
    for item in items {
        match item {
            Item::Fill(fill) => {
                renderer.fill_shape(transform * &fill.path, fill.color);
            }
            Item::Stroke(stroke) => {
                renderer.stroke(
                    transform * &stroke.path,
                    &kurbo::Stroke {
                        width: stroke.width,
                        ..kurbo::Stroke::default()
                    },
                    stroke.color,
                );
            }
            Item::Group(group) => {
                encode_svg(renderer, transform * group.affine, &group.children);
            }
        }
    }
}
