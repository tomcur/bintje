use peniko::{
    color::{PremulColor, PremulRgba8},
    BrushRef,
};

use crate::{Strip, Tile};

/// Number of tiles per wide tile.
pub(crate) const WIDE_TILE_WIDTH_TILES: u16 = 16;

/// Number of pixels per wide tile.
pub(crate) const WIDE_TILE_WIDTH_PX: u16 = Tile::WIDTH * WIDE_TILE_WIDTH_TILES;

#[derive(Debug)]
pub(crate) enum Command {
    /// Fill the paint buffer with an opaque color.
    // Fill(Fill),
    ///
    Sample(Sample),

    /// A fill between two strips.
    SparseFill(SparseFill),

    #[expect(unused, reason = "TODO")]
    PushClip(()),
    #[expect(unused, reason = "TODO")]
    PopClip(()),
}

#[derive(Debug)]
pub(crate) struct Sample {
    /// Offset within the wide tile, in tiles.
    x: u16,
    /// The width of the wide tile, in tiles.
    width: u16,
    color: PremulRgba8,
    alpha_idx: u32,
}

#[derive(Debug)]
pub(crate) struct SparseFill {
    x: u16,
    width: u16,
    color: PremulRgba8,
}

#[derive(Debug)]
pub(crate) struct WideTile {
    pub commands: Vec<Command>,
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

/// img is Rgba8
pub(crate) fn render(
    width: u16,
    height: u16,
    img: &mut [PremulRgba8],
    alpha_masks: &[u8],
    wide_tiles: &[WideTile],
) {
    assert_eq!(img.len(), width as usize * height as usize);
    assert_eq!(
        wide_tiles.len(),
        width.div_ceil(WIDE_TILE_WIDTH_PX) as usize * height.div_ceil(Tile::HEIGHT) as usize
    );

    let mut wide_tile_idx = 0;
    for wide_tile_y in 0..height.div_ceil(Tile::HEIGHT) {
        for wide_tile_x in 0..width.div_ceil(WIDE_TILE_WIDTH_PX) {
            // Debug-render a wide tile checkerboard backdrop
            let dark_wide_tile = (wide_tile_y & 1) != (wide_tile_x & 1);

            let wide_tile = &wide_tiles[wide_tile_idx];
            wide_tile_idx += 1;

            for y in 0..Tile::HEIGHT {
                let img_y = wide_tile_y * Tile::HEIGHT + y;

                for x in 0..WIDE_TILE_WIDTH_TILES * Tile::WIDTH {
                    let img_x = wide_tile_x * WIDE_TILE_WIDTH_PX + x;
                    let img_idx = img_y as usize * width as usize + img_x as usize;

                    if img_y >= height || img_x >= width {
                        continue;
                    }

                    if dark_wide_tile {
                        img[img_idx] = PremulRgba8 {
                            r: 220,
                            g: 220,
                            b: 200,
                            a: 255,
                        };
                    } else {
                        img[img_idx] = PremulRgba8 {
                            r: 240,
                            g: 240,
                            b: 220,
                            a: 255,
                        };
                    }
                }
            }

            for command in wide_tile.commands.iter() {
                match command {
                    Command::Sample(sample) => {
                        for y in 0..Tile::HEIGHT {
                            let img_y = wide_tile_y * Tile::HEIGHT + y;

                            for x in 0..sample.width * Tile::WIDTH {
                                let img_x =
                                    sample.x * Tile::WIDTH + wide_tile_x * WIDE_TILE_WIDTH_PX + x;

                                if img_y >= height || img_x >= width {
                                    continue;
                                }

                                let img_idx = img_y as usize * width as usize + img_x as usize;
                                // let alpha_idx = sample.alpha_idx as usize
                                //     + y as usize
                                //         * sample.width as usize
                                //         * crate::tile::TILE_WIDTH as usize
                                //     + x as usize;
                                let alpha_idx = sample.alpha_idx as usize
                                    + x as usize * Tile::HEIGHT as usize
                                    + y as usize;
                                let composite_color =
                                    mul_alpha(sample.color, alpha_masks[alpha_idx]);
                                img[img_idx] = over(img[img_idx], composite_color);
                            }
                        }
                    }
                    Command::SparseFill(sparse_fill) => {
                        for y in 0..Tile::HEIGHT {
                            let img_y = wide_tile_y * Tile::HEIGHT + y;

                            for x in 0..sparse_fill.width * Tile::WIDTH {
                                let img_x = sparse_fill.x * Tile::WIDTH
                                    + wide_tile_x * WIDE_TILE_WIDTH_PX
                                    + x;

                                if img_y >= height || img_x >= width {
                                    continue;
                                }

                                let img_idx = img_y as usize * width as usize + img_x as usize;
                                img[img_idx] = over(img[img_idx], sparse_fill.color);
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }
}

/// Multiply the alpha over a color.
fn mul_alpha(color: PremulRgba8, alpha: u8) -> PremulRgba8 {
    (PremulColor::from(color) * (alpha as f32 * (1. / 255.))).to_rgba8()
}

/// Composite one color over another.
fn over(under: PremulRgba8, over: PremulRgba8) -> PremulRgba8 {
    let under = PremulColor::from(under);
    let over = PremulColor::from(over);

    let mut composite = over + under * (1. - over.components[3]);
    composite.components[3] = over.components[3] + under.components[3] * (1. - over.components[3]);
    composite.to_rgba8()
}
