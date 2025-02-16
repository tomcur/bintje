//! A Bintje example rendering an SVG.

use std::path::Path;

use bintje::Bintje;
use kurbo::Affine;
use pico_svg::Item;

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

    let file = std::fs::OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open("test.png")
        .unwrap();
    renderer.to_png(file).unwrap();
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
