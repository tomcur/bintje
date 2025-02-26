use crate::{Line, Tile, TileRow};

/// A strip of merged tiles.
///
/// Strips are the same height as tiles, but are an integer multiple of a tile's width.
#[derive(Clone, Copy, Debug)]
pub struct Strip {
    /// The top-left coordinate of the strip in tiles.
    pub x: u16,
    /// The top-left coordinate of the strip in tiles.
    pub y: u16,
    /// The width of the strip (in number of tiles).
    pub width: u16,

    /// The per-pixel area coverage between the end of the previous strip (i.e., right-most edge)
    /// and the start of this strip (i.e., left-most edge).
    ///
    /// If this is the first strip, the area covered is between the viewport's left edge and this
    /// strip.
    pub pixel_coverage: [u8; Tile::HEIGHT as usize],

    /// The index of the strip into the alpha mask storage.
    pub alpha_idx: u32,
}

/// `tiles` must be in (y, x) sorted order.
#[inline(never)]
pub(crate) fn generate_strips(
    row: &TileRow,
    row_y: u16,
    lines: &[Line],
    alpha_storage: &mut Vec<u8>,
    strips: &mut Vec<Strip>,
) {
    if row.tiles.is_empty() || lines.is_empty() {
        return;
    }

    // The accumulated tile winding delta. A line that crosses the top edge of a tile
    // increments the delta if the line is directed upwards, and decrements it if goes
    // downwards. Horizontal lines leave it unchanged.
    let mut winding_delta: i32 = row.winding;

    // The previous tile visited.
    let mut prev_tile = row.tiles[0];
    // The accumulated (fractional) winding of the tile-sized location we're currently at:
    // multiple tiles can be at the same location.
    let mut location_winding = [row.area_coverage; Tile::WIDTH as usize];
    // The accumulated (fractional) windings at this location's right edge. When we move to the
    // next location, this is splatted to that location's starting winding.
    let mut accumulated_winding = row.area_coverage;

    let row_top_y = (row_y * Tile::HEIGHT) as f32;

    /// A special tile to keep the logic below simple.
    const GATE_CLOSER: Tile = Tile {
        x: u16::MAX,
        line_idx: 0,
    };

    // The strip we're building.
    let mut strip = Strip {
        x: prev_tile.x,
        y: row_y,
        width: 0,
        pixel_coverage: row
            .area_coverage
            .map(|coverage| (coverage.abs() * u8::MAX as f32).round() as u8),
        alpha_idx: alpha_storage.len() as u32,
    };

    for tile in row.tiles.iter().copied().chain([GATE_CLOSER]) {
        // Push out the winding as an alpha mask when we move to the next location (i.e., a tile
        // without the same location).
        if prev_tile.x < tile.x {
            #[expect(clippy::needless_range_loop, reason = "Clarity")]
            for x in 0..Tile::WIDTH as usize {
                for y in 0..Tile::HEIGHT as usize {
                    // TODO(Tom): even-odd winding.
                    alpha_storage
                        .push((location_winding[x][y].abs() * u8::MAX as f32).round() as u8);
                }
                location_winding[x] = accumulated_winding;
            }
        }

        // Push out the strip if we're moving to a next strip.
        if prev_tile.x + 1 < tile.x {
            strip.width = prev_tile.x - strip.x + 1;
            strips.push(strip);
            strip = Strip {
                x: tile.x,
                y: row_y,
                width: 0,
                pixel_coverage: accumulated_winding
                    .map(|coverage| (coverage.abs() * u8::MAX as f32).round() as u8),
                alpha_idx: alpha_storage.len() as u32,
            };
            // Note: this fill is mathematically not necessary. It provides a way to reduce
            // accumulation of float round errors.
            // TODO(Tom): since horizontal geometry is elided, we'd need to track (on tiles?)
            // whether there was any horizontal geometry here. Without that, we can't easily know
            // here currently if per-pixel winding is equal to the coarse winding.
            // accumulated_winding.fill(winding_delta as f32);

            // TODO: maybe just push out the strip manually at the end, rather than this?
            if tile.x == u16::MAX {
                break;
            }
        }
        prev_tile = tile;

        let tile_left_x = (tile.x * Tile::WIDTH) as f32;

        let line = lines[tile.line_idx as usize];
        let p0_x = line.p0.x - tile_left_x;
        let p0_y = line.p0.y - row_top_y;
        let p1_x = line.p1.x - tile_left_x;
        let p1_y = line.p1.y - row_top_y;

        let sign = (p0_y - p1_y).signum();

        // Calculate winding / pixel area coverage.
        //
        // Conceptually, horizontal rays are shot from left to the right. Every time the ray
        // crosses a line that is directed upwards (decreasing `y`), the winding is
        // incremented. Every time the ray crosses a line moving downwards (increasing `y`),
        // the winding is decremented. The fractional area coverage of a pixel is the integral
        // of the winding within it.
        //
        // Practically, to calculate this, each pixel is considered individually, and we
        // determine whether the line moves through this pixel. The line's y-delta within this
        // pixel is is accumulated and added to the area coverage of pixels to the right.
        // Within the pixel itself, the area to the right of the line segment forms a trapezoid
        // (or a triangle in the degenerate case). The area of this trapezoid is added to the
        // pixel's area coverage. For lines directed upwards, the area is positive, and
        // negative for lines direct downwards.

        let (line_top_y, line_top_x, line_bottom_y, line_bottom_x) = if p0_y < p1_y {
            (p0_y, p0_x, p1_y, p1_x)
        } else {
            (p1_y, p1_x, p0_y, p0_x)
        };

        let y_slope = (line_bottom_y - line_top_y) / (line_bottom_x - line_top_x);
        let x_slope = 1. / y_slope;

        {
            // The y-coordinate of the intersections between line and the tile's left and right
            // edges respectively.
            //
            // There's some subtety goin on here, see the note on `line_px_left_y` below.
            let line_tile_left_y = (line_top_y - line_top_x * y_slope)
                .max(line_top_y)
                .min(line_bottom_y);
            let line_tile_right_y = (line_top_y + (Tile::WIDTH as f32 - line_top_x) * y_slope)
                .max(line_top_y)
                .min(line_bottom_y);

            winding_delta +=
                sign as i32 * (line_tile_left_y.signum() != line_tile_right_y.signum()) as i32;
        }

        for y_idx in 0..Tile::HEIGHT {
            let px_top_y = y_idx as f32;
            let px_bottom_y = 1. + y_idx as f32;

            let ymin = f32::max(line_top_y, px_top_y);
            let ymax = f32::min(line_bottom_y, px_bottom_y);

            let mut acc = 0.;
            for x_idx in 0..Tile::WIDTH {
                let px_left_x = x_idx as f32;
                let px_right_x = 1. + x_idx as f32;

                // The y-coordinate of the intersections between line and the pixel's left and
                // right edge's respectively.
                //
                // There is some subtlety going on here: `y_slope` will usually be finite, but will
                // be `inf` for purely vertical lines (`p0_x == p1_x`).
                //
                // In the case of `inf`, the resulting slope calculation will be `-inf` or `inf`
                // depending on whether the pixel edge is left or right of the line, respectively
                // (from the viewport's coordinate system perspective). The `min` and `max`
                // y-clamping logic generalizes nicely, as a pixel edge to the left of the line is
                // clamped to `ymin`, and a pixel edge to the right is clamped to `ymax`.
                let line_px_left_y = (line_top_y + (px_left_x - line_top_x) * y_slope)
                    .max(ymin)
                    .min(ymax);
                let line_px_right_y = (line_top_y + (px_right_x - line_top_x) * y_slope)
                    .max(ymin)
                    .min(ymax);

                // `x_slope` is always finite, as horizontal geometry is elided.
                let line_px_left_yx = line_top_x + (line_px_left_y - line_top_y) * x_slope;
                let line_px_right_yx = line_top_x + (line_px_right_y - line_top_y) * x_slope;
                let h = (line_px_right_y - line_px_left_y).abs();
                let area =
                    0.5 * h * ((px_right_x - line_px_right_yx) + (px_right_x - line_px_left_yx));
                location_winding[x_idx as usize][y_idx as usize] += acc + sign * area;
                acc += sign * h;
            }
            accumulated_winding[y_idx as usize] += acc;
        }
    }
}
