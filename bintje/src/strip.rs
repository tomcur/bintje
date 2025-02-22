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

    /// The winding of the path at the end (i.e, right-most edge) of the strip.
    ///
    /// A line that crosses the top edge of a tile increments the delta if the line is directed
    /// upwards, and decrements it if goes downwards. Horizontal lines leave it unchanged.
    pub winding: i32,

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

    // The index of the strip we're currently building into the alpha mask storage.
    let mut alpha_idx = alpha_storage.len();

    // The first tile of the strip we're currently building.
    let mut first_tile = row.tiles[0];
    // The previous tile visited.
    let mut prev_tile = row.tiles[0];
    // The accumulated (fractional) winding of the tile-sized location we're currently at:
    // multiple tiles can be at the same location.
    let mut location_winding = [row.area_coverage; Tile::WIDTH as usize];
    // The accumulated (fractional) windings at this location's right edge. When we move to the
    // next location, this is splatted to that location's starting winding.
    let mut accumulated_winding = [0f32; Tile::HEIGHT as usize];

    let row_top_y = (row_y * Tile::HEIGHT) as f32;

    /// A special tile to keep the logic below simple.
    const GATE_CLOSER: Tile = Tile {
        x: u16::MAX,
        line_idx: 0,
    };

    for tile in row.tiles.iter().copied().chain([GATE_CLOSER]) {
        // Push out the winding as an alpha mask when we move to the next location (i.e., a tile
        // without the same location).
        if prev_tile.x < tile.x {
            #[expect(clippy::needless_range_loop, reason = "Clarity")]
            for x in 0..Tile::WIDTH as usize {
                for y in 0..Tile::HEIGHT as usize {
                    // TODO(Tom): even-odd winding.
                    // TODO(Tom): does this need adjusting for the target color space's
                    // transfer function?
                    alpha_storage
                        .push((location_winding[x][y].abs() * u8::MAX as f32).round() as u8);
                }
                location_winding[x] = accumulated_winding;
            }
        }

        // Push out the strip if we're moving to a next strip.
        if prev_tile.x + 1 < tile.x {
            let strip = Strip {
                x: first_tile.x,
                y: row_y,
                width: prev_tile.x - first_tile.x + 1,
                winding: winding_delta,
                alpha_idx: alpha_idx as u32,
            };
            strips.push(strip);
            first_tile = tile;
            alpha_idx = alpha_storage.len();
            // Note: this fill is mathematically not necessary. It provides a way to reduce
            // accumulation of float round errors.
            accumulated_winding.fill(winding_delta as f32);

            // TODO: maybe just push out the strip manually at the end, rather than this?
            if tile.x == u16::MAX {
                break;
            }
        }
        prev_tile = tile;

        let line = lines[tile.line_idx as usize];
        let p0_x = line.p0.x;
        let p0_y = line.p0.y;
        let p1_x = line.p1.x;
        let p1_y = line.p1.y;

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

        let (line_left_x, line_left_y, line_right_x, line_right_y) = if p0_x < p1_x {
            (p0_x, p0_y, p1_x, p1_y)
        } else {
            (p1_x, p1_y, p0_x, p0_y)
        };
        let (line_top_y, line_bottom_y) = if p0_y < p1_y {
            (p0_y, p1_y)
        } else {
            (p1_y, p0_y)
        };

        let y_slope = (line_right_y - line_left_y) / (line_right_x - line_left_x);

        let tile_left_y = (line_left_y
            + (tile.x as f32 * Tile::WIDTH as f32 - line_left_x) * y_slope)
            .max(line_top_y)
            .min(line_bottom_y);
        let tile_right_y = (line_left_y
            + ((tile.x + 1) as f32 * Tile::WIDTH as f32 - line_left_x) * y_slope)
            .max(line_top_y)
            .min(line_bottom_y);

        let ymin = f32::min(tile_left_y, tile_right_y);
        let ymax = f32::max(tile_left_y, tile_right_y);
        winding_delta += sign as i32 * (ymin <= row_top_y && ymax > row_top_y) as i32;

        // Currently differently parameterized from y_slope.
        // I feel like there's a smarter order of doing things that would be faster...
        let x_slope = (p1_x - p0_x) / (p1_y - p0_y);
        for y_idx in 0..Tile::HEIGHT {
            let y = row_top_y + y_idx as f32;

            let ymin = line_top_y.max(y).min(y + 1.);
            let ymax = line_bottom_y.max(y).min(y + 1.);

            let mut y_right = tile_left_y.max(ymin).min(ymax);
            let mut y_right_x = p0_x + (y_right - p0_y) * x_slope;

            let mut acc = 0.;
            // // TODO(Tom): reduce operations by taking the previous iteration's `y_right` as the
            // // current iteration's `y_next`.
            // // 2025-02-17: It appears not to help in the 4x4 case.
            // // 2025-02-18: It actually turns out to be every so slightly faster, but ideally more
            // // principled measurements would be performed.
            // //
            // // TODO(Tom): does short-circuiting help? e.g., if both x coordinates are to the
            // // left of this pixel's right edge, breaking this inner loop?
            // // 2025-02-17: It appears not to help in the 4x4 case.
            for x_idx in 0..Tile::WIDTH {
                let x = (tile.x * Tile::WIDTH + x_idx) as f32;

                let y_left = y_right;
                y_right = (line_left_y + (x + 1. - line_left_x) * y_slope)
                    .max(ymin)
                    .min(ymax);

                let y_left_x = y_right_x;
                y_right_x = p0_x + (y_right - p0_y) * x_slope;

                let h = (y_left - y_right).abs();
                let area = 0.5 * h * (x + x + 2. - y_left_x - y_right_x);
                location_winding[x_idx as usize][y_idx as usize] += acc + sign * area.max(0.);
                acc += sign * h;
            }
            accumulated_winding[y_idx as usize] += acc;
        }
    }
}
