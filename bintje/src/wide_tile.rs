use peniko::{
    color::{PremulColor, PremulRgba8},
    BrushRef,
};

use crate::{Strip, Tile};

/// Number of tiles per wide tile.
pub(crate) const WIDE_TILE_WIDTH_TILES: u16 = 32;

/// Number of pixels per wide tile.
pub(crate) const WIDE_TILE_WIDTH_PX: u16 = Tile::WIDTH * WIDE_TILE_WIDTH_TILES;

#[derive(Debug)]
pub enum Command {
    /// A fill sampling from an alpha mask.
    Sample(Sample),

    /// An opaque fill between two strips.
    SparseFill(SparseFill),

    /// TODO(Tom).
    PushClip(()),
    /// TODO(Tom).
    PopClip(()),
}

#[derive(Debug)]
pub struct Sample {
    /// The offset within the wide tile, in tiles.
    pub x: u16,
    /// The width of the area to be filled, in tiles.
    pub width: u16,
    pub color: PremulRgba8,
    /// The index into the global alpha mask, encoding the pixel coverage of the area to be filled.
    pub alpha_idx: u32,
}

#[derive(Debug)]
pub struct SparseFill {
    pub x: u16,
    pub width: u16,
    pub color: PremulRgba8,
}

#[derive(Debug)]
pub struct WideTile {
    pub commands: Vec<Command>,
}

impl WideTile {
    /// Number of tiles per wide tile.
    pub const WIDTH_TILES: u16 = WIDE_TILE_WIDTH_TILES;

    /// Number of pixels per wide tile.
    pub const WIDTH_PX: u16 = WIDE_TILE_WIDTH_PX;
}

pub(crate) fn generate_wide_tile_commands<'b>(
    width: u16,
    wide_tiles: &mut [WideTile],
    strips: &[Strip],
    brush: impl Into<peniko::BrushRef<'b>>,
) {
    let brush = brush.into();
    let wide_tile_columns = width.div_ceil(WIDE_TILE_WIDTH_PX);
    let wide_tile_rows = (wide_tiles.len() / wide_tile_columns as usize) as u16;

    let mut prev_strip = Strip {
        x: 0,
        y: u16::MAX,
        width: 0,
        winding: 0,
        alpha_idx: 0,
    };

    for strip in strips.iter().copied() {
        let wide_tile_x = strip.x / WIDE_TILE_WIDTH_TILES;
        let wide_tile_y = strip.y;

        if wide_tile_y >= wide_tile_rows {
            break;
        }

        let color = match brush {
            BrushRef::Solid(color) => color,
            _ => peniko::color::palette::css::RED,
        };

        // Command sparse fills.
        if prev_strip.winding != 0
            && prev_strip.y == strip.y
            && prev_strip.x + prev_strip.width < strip.x + 1
        {
            let start_wide_tile_x = (prev_strip.x + prev_strip.width) / WIDE_TILE_WIDTH_TILES;
            let end_wide_tile_x = strip.x / WIDE_TILE_WIDTH_TILES;
            for wide_tile_x in start_wide_tile_x..=end_wide_tile_x {
                if wide_tile_x >= wide_tile_columns {
                    break;
                }

                let x_start = if wide_tile_x == start_wide_tile_x {
                    prev_strip.x + prev_strip.width - start_wide_tile_x * WIDE_TILE_WIDTH_TILES
                } else {
                    0
                };

                let x_end = if wide_tile_x == end_wide_tile_x {
                    strip.x - end_wide_tile_x * WIDE_TILE_WIDTH_TILES
                } else {
                    WIDE_TILE_WIDTH_TILES
                };

                let wide_tile = wide_tiles
                    .get_mut((wide_tile_y * wide_tile_columns + wide_tile_x) as usize)
                    .unwrap();
                wide_tile.commands.push(Command::SparseFill(SparseFill {
                    x: x_start,
                    width: x_end - x_start,
                    color: color.premultiply().to_rgba8(),
                }));
            }
        }

        // Command alpha mask samples.
        let start_wide_tile_x = wide_tile_x;
        let end_wide_tile_x = (strip.x + strip.width) / WIDE_TILE_WIDTH_TILES;
        let mut alpha_idx = strip.alpha_idx;

        for wide_tile_x in start_wide_tile_x..=end_wide_tile_x {
            if wide_tile_x >= wide_tile_columns {
                break;
            }

            let x_start = if wide_tile_x == start_wide_tile_x {
                strip.x - start_wide_tile_x * WIDE_TILE_WIDTH_TILES
            } else {
                0
            };

            let x_end = if wide_tile_x == end_wide_tile_x {
                strip.x + strip.width - end_wide_tile_x * WIDE_TILE_WIDTH_TILES
            } else {
                WIDE_TILE_WIDTH_TILES
            };
            let width = x_end - x_start;

            let wide_tile = wide_tiles
                .get_mut((wide_tile_y * wide_tile_columns + wide_tile_x) as usize)
                .unwrap();

            wide_tile.commands.push(Command::Sample(Sample {
                x: x_start,
                width: x_end - x_start,
                color: color.premultiply().to_rgba8(),
                alpha_idx,
            }));
            alpha_idx += width as u32 * Tile::WIDTH as u32 * Tile::HEIGHT as u32;
        }

        prev_strip = strip;
    }
}

/// CPU rasterization of draw commands to a pixel buffer.
pub fn cpu_rasterize(
    width: u16,
    height: u16,
    img: &mut [PremulRgba8],
    alpha_masks: &[u8],
    wide_tiles: &[WideTile],
) {
    const PRINT_CHECKERBOARD: bool = false;

    assert_eq!(img.len(), width as usize * height as usize);
    assert_eq!(
        wide_tiles.len(),
        width.div_ceil(WIDE_TILE_WIDTH_PX) as usize * height.div_ceil(Tile::HEIGHT) as usize
    );

    let wide_tile_rows = height.div_ceil(Tile::HEIGHT);
    let wide_tile_columns = width.div_ceil(WIDE_TILE_WIDTH_PX);

    let mut wide_tile_idx = 0;
    for wide_tile_y in 0..wide_tile_rows {
        for wide_tile_x in 0..wide_tile_columns {
            let wide_tile = &wide_tiles[wide_tile_idx];
            wide_tile_idx += 1;

            let mut scratch =
                [PremulRgba8::from_u32(0); WIDE_TILE_WIDTH_PX as usize * Tile::HEIGHT as usize];

            if PRINT_CHECKERBOARD {
                // Debug-render a wide tile checkerboard backdrop
                let dark_wide_tile = (wide_tile_y & 1) != (wide_tile_x & 1);
                if dark_wide_tile {
                    scratch.fill(PremulRgba8 {
                        r: 220,
                        g: 220,
                        b: 200,
                        a: 255,
                    });
                } else {
                    scratch.fill(PremulRgba8 {
                        r: 240,
                        g: 240,
                        b: 220,
                        a: 255,
                    });
                }
            }

            for command in wide_tile.commands.iter() {
                match command {
                    Command::Sample(sample) => {
                        for y in 0..Tile::HEIGHT {
                            // let img_y = wide_tile_y * Tile::HEIGHT + y;
                            let mut idx = y as usize * WIDE_TILE_WIDTH_PX as usize
                                + (sample.x * Tile::WIDTH) as usize;

                            for x in 0..sample.width * Tile::WIDTH {
                                let alpha_idx = sample.alpha_idx as usize
                                    + x as usize * Tile::HEIGHT as usize
                                    + y as usize;
                                let composite_color =
                                    mul_alpha(sample.color, alpha_masks[alpha_idx]);
                                scratch[idx] = over(scratch[idx], composite_color);
                                idx += 1;
                            }
                        }
                    }
                    Command::SparseFill(sparse_fill) => {
                        for y in 0..Tile::HEIGHT {
                            let mut idx = y as usize * WIDE_TILE_WIDTH_PX as usize
                                + (sparse_fill.x * Tile::WIDTH) as usize;

                            if sparse_fill.color.a == 255 {
                                // Opaque colors do not need compositing.
                                scratch[idx..idx + (sparse_fill.width * Tile::WIDTH) as usize]
                                    .fill(sparse_fill.color);
                            } else {
                                for _ in 0..sparse_fill.width * Tile::WIDTH {
                                    scratch[idx] = over(scratch[idx], sparse_fill.color);
                                    idx += 1;
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }

            let mut img_y = wide_tile_y * Tile::HEIGHT;
            for y in 0..Tile::HEIGHT {
                let mut img_x = wide_tile_x * WIDE_TILE_WIDTH_PX;
                let mut img_idx = img_y as usize * width as usize + img_x as usize;
                if img_y >= height {
                    break;
                }
                if wide_tile_x + 1 < wide_tile_columns {
                    let scratch_idx = y as usize * WIDE_TILE_WIDTH_PX as usize;
                    img[img_idx..img_idx + WIDE_TILE_WIDTH_PX as usize].copy_from_slice(
                        &scratch[scratch_idx..scratch_idx + WIDE_TILE_WIDTH_PX as usize],
                    );
                } else {
                    for x in 0..WIDE_TILE_WIDTH_PX {
                        if img_x >= width {
                            break;
                        }
                        img[img_idx] =
                            scratch[y as usize * WIDE_TILE_WIDTH_PX as usize + x as usize];

                        img_x += 1;
                        img_idx += 1;
                    }
                }

                img_y += 1;
            }
        }
    }
}

/// Multiply the alpha over a color.
fn mul_alpha(color: PremulRgba8, alpha: u8) -> PremulRgba8 {
    const COMPOSITE_IN_F32: bool = false;

    if COMPOSITE_IN_F32 {
        (PremulColor::from(color) * (alpha as f32 * (1. / 255.))).to_rgba8()
    } else {
        let mut arr = color.to_u8_array();
        for component in &mut arr {
            *component = ((*component as u16 * alpha as u16) / 255) as u8;
        }
        PremulRgba8::from_u8_array(arr)
    }
}

/// Composite one color over another.
fn over(under: PremulRgba8, over: PremulRgba8) -> PremulRgba8 {
    const COMPOSITE_IN_F32: bool = false;

    if COMPOSITE_IN_F32 {
        let under = PremulColor::from(under);
        let over = PremulColor::from(over);

        let mut composite = over + under * (1. - over.components[3]);
        composite.components[3] =
            over.components[3] + under.components[3] * (1. - over.components[3]);
        composite.to_rgba8()
    } else {
        let mut under = under.to_u8_array();
        let over = over.to_u8_array();

        for idx in 0..3 {
            under[idx] =
                ((over[idx] as u16 * 255 + under[idx] as u16 * (255 - over[3]) as u16) / 255) as u8;
        }
        under[3] = ((over[3] as u16 * 255 + under[3] as u16 * (255 - over[3] as u16)) / 255) as u8;

        PremulRgba8::from_u8_array(under)
    }
}
