use std::path::Path;

use bytemuck::Zeroable;
use color::PremulRgba8;
use image::ImageEncoder;
use kurbo::Shape;

use crate::{wide_tile, Bintje};

// Creates a new instance of TestEnv and put current function name in constructor
#[macro_export]
macro_rules! testenv {
    () => {{
        // Get name of the current function
        fn f() {}
        fn type_name_of<T>(_: T) -> &'static str {
            std::any::type_name::<T>()
        }
        let name = type_name_of(f);
        let name = &name[..name.len() - 3];
        let name = &name[name.rfind(':').map(|x| x + 1).unwrap_or(0)..];

        // Create test env
        $crate::tests::TestEnv::new(name.to_string())
    }};
}

struct TestEnv {
    name: String,
    counter: u8,
    bintje: Option<Bintje>,
    img: Vec<PremulRgba8>,
}

impl TestEnv {
    pub(crate) fn new(name: String) -> Self {
        TestEnv {
            name,
            counter: 0,
            bintje: None,
            img: Vec::new(),
        }
    }

    /// Set the size of the render context. This resets the renderer state.
    pub fn set_size(&mut self, width: u16, height: u16) {
        self.bintje = Some(Bintje::new(width, height));
    }

    /// Get the render context. Call `self.set_size` first.
    pub fn renderer(&mut self) -> &mut Bintje {
        self.bintje
            .as_mut()
            .expect("Call `TestEnv::set_size` first.")
    }

    /// Rasterize the current render context to a PNG file, with the name based on the test
    /// environment.
    pub fn rasterize_to_png(&mut self) {
        let renderer = self
            .bintje
            .as_ref()
            .expect("Call `TestEnv::set_size` first.");
        let (width, height) = renderer.size();
        self.img.clear();
        self.img
            .resize(width as usize * height as usize, PremulRgba8::zeroed());
        let commands = renderer.commands();
        wide_tile::cpu_rasterize(
            width,
            height,
            &mut self.img,
            commands.alpha_masks,
            commands.wide_tiles,
        );

        let img_name = if self.counter == 0 {
            format!("{}.png", &self.name)
        } else {
            format!("{}-{}.png", &self.name, self.counter)
        };
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/current")
            .join(img_name);
        self.counter.checked_add(1).unwrap();

        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(&path)
            .unwrap();
        let encoder = image::codecs::png::PngEncoder::new(&mut file);
        encoder
            .write_image(
                bytemuck::cast_slice(&self.img),
                width as u32,
                height as u32,
                image::ExtendedColorType::Rgba8,
            )
            .unwrap();
    }
}

#[test]
fn triangular_stroke() {
    let mut env = testenv!();
    env.set_size(64, 64);

    let renderer = env.renderer();
    renderer.stroke(
        kurbo::Triangle::new((8., 4.), (20., 50.), (55., 45.)).path_elements(f64::NAN),
        &kurbo::Stroke {
            width: 3.5,
            ..kurbo::Stroke::default()
        },
        color::palette::css::ORANGE_RED,
    );
    env.rasterize_to_png();
}

#[test]
fn composite() {
    let mut env = testenv!();
    env.set_size(128, 128);

    let renderer = env.renderer();
    renderer.fill_shape(
        kurbo::Rect::new(25., 15., 110., 120.),
        peniko::color::palette::css::BLUE.with_alpha(1.0),
    );
    renderer.fill_shape(
        kurbo::Triangle::new((68., 20.), (101., 99.), (34., 107.)),
        peniko::color::palette::css::GREEN.with_alpha(1.0),
    );
    renderer.fill_shape(
        kurbo::Circle::new((50., 50.), 45.),
        peniko::color::palette::css::RED.with_alpha(0.5),
    );
    env.rasterize_to_png();
}

#[test]
fn overflow_left_viewport() {
    let mut env = testenv!();
    env.set_size(64, 64);

    let renderer = env.renderer();
    renderer.stroke(
        kurbo::Rect::new(-0.5, 5.5, 50.5, 40.5).path_elements(f64::NAN),
        &kurbo::Stroke {
            width: 1.0,
            ..kurbo::Stroke::default()
        },
        color::palette::css::ORANGE_RED,
    );
    env.rasterize_to_png();
}
