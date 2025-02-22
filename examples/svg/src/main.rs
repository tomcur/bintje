//! A Bintje example rendering an SVG.

use std::path::Path;

use image::ImageEncoder;
use kurbo::Affine;
use peniko::color::{self, PremulRgba8};
use pico_svg::Item;

use bintje::{cpu_rasterize, Bintje};
use bintje_wgpu::RenderContext;

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
    let (width, height) = renderer.size();

    let mut gpu_render_context = bintje_wgpu::block_on(RenderContext::create());
    let mut fragment_shader = gpu_render_context.rasterizer(width, height);

    let mut img = vec![PremulRgba8::from_u32(0); width as usize * height as usize];
    let now = std::time::Instant::now();
    let mut coarse = std::time::Duration::ZERO;
    let mut fine = std::time::Duration::ZERO;
    const NUM_ITERATIONS: u16 = 100;
    for _ in 0..NUM_ITERATIONS {
        renderer.clear();
        let mut start = std::time::Instant::now();
        encode_svg(&mut renderer, 1. / scale, Affine::IDENTITY, &svg.items);
        coarse += start.elapsed();
        start = std::time::Instant::now();
        let commands = renderer.commands();
        // cpu_rasterize(
        //     width,
        //     height,
        //     &mut img,
        //     commands.alpha_masks,
        //     commands.wide_tiles,
        // );
        fragment_shader.rasterize(
            commands.alpha_masks,
            commands.wide_tiles,
            width,
            bytemuck::cast_slice_mut(&mut img),
        );
        fine += start.elapsed();
    }
    println!(
        "Total elapsed:                  {:?}ms",
        now.elapsed().as_nanos() as f32 / (NUM_ITERATIONS as f32 * 1_000_000.)
    );
    println!(
        "Coarse elapsed:                 {:?}ms",
        coarse.as_nanos() as f32 / (NUM_ITERATIONS as f32 * 1_000_000.)
    );
    println!(
        " - Elapsed flattening:          {:?}ms",
        renderer.flattening_time.as_nanos() as f32 / (NUM_ITERATIONS as f32 * 1_000_000.)
    );
    println!(
        " - Elapsed flattening (stroke): {:?}ms",
        renderer.flattening_stroke_time.as_nanos() as f32 / (NUM_ITERATIONS as f32 * 1_000_000.)
    );
    println!(
        " - Stripping elapsed:           {:?}ms",
        renderer.strip_generation_time.as_nanos() as f32 / (NUM_ITERATIONS as f32 * 1_000_000.)
    );
    println!(
        " - Tile generation elapsed:     {:?}ms",
        renderer.tile_generation_time.as_nanos() as f32 / (NUM_ITERATIONS as f32 * 1_000_000.)
    );
    println!(
        " - Tile sorting elapsed:        {:?}ms",
        renderer.tile_sorting_time.as_nanos() as f32 / (NUM_ITERATIONS as f32 * 1_000_000.)
    );
    println!(
        "Fine elapsed:                   {:?}ms",
        fine.as_nanos() as f32 / (NUM_ITERATIONS as f32 * 1_000_000.)
    );

    unpremultiply(&mut img);
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

fn unpremultiply(premultiplied: &mut [PremulRgba8]) {
    for color in premultiplied {
        let f_color = color::PremulColor::<color::Srgb>::from(*color);
        let rgba8 = f_color.un_premultiply().to_rgba8();
        *color = PremulRgba8 {
            r: rgba8.r,
            g: rgba8.g,
            b: rgba8.b,
            a: rgba8.a,
        };
    }
}

fn encode_svg(renderer: &mut Bintje, scale_recip: f64, transform: Affine, items: &[Item]) {
    renderer.push_transform(transform);
    for item in items {
        match item {
            Item::Fill(fill) => {
                renderer.fill_shape(&fill.path, fill.color);
            }
            Item::Stroke(stroke) => {
                renderer.stroke(
                    &stroke.path,
                    &kurbo::Stroke {
                        width: stroke.width * scale_recip,
                        ..kurbo::Stroke::default()
                    },
                    stroke.color,
                );
            }
            Item::Group(group) => {
                encode_svg(renderer, scale_recip, group.affine, &group.children);
            }
        }
    }
    renderer.pop_transform();
}
