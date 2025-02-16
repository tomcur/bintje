use crate::{tile::TilePoint, Tile};

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
pub(crate) fn generate_strips(
    tiles: &[Tile],
    alpha_storage: &mut Vec<u8>,
    strips: &mut Vec<Strip>,
) {
    if tiles.is_empty() {
        return;
    }

    // The accumulated tile winding delta. A line that crosses the top edge of a tile
    // increments the delta if the line is directed upwards, and decrements it if goes
    // downwards. Horizontal lines leave it unchanged.
    let mut winding_delta: i32 = 0;

    // The index of the strip we're currently building into the alpha mask storage.
    let mut alpha_idx = alpha_storage.len();

    // The first tile of the strip we're currently building.
    let mut first_tile = tiles[0];
    // The previous tile visited.
    let mut prev_tile = tiles[0];
    // The accumulated (fractional) winding of the tile-sized location we're currently at:
    // multiple tiles can be at the same location.
    let mut location_winding = [[0f32; Tile::HEIGHT as usize]; Tile::WIDTH as usize];
    // The accumulated (fractional) windings at this location's right edge. When we move to the
    // next location, this is splatted to that location's starting winding.
    let mut accumulated_winding = [0f32; Tile::HEIGHT as usize];

    /// A special tile to keep the logic below simple.
    const GATE_CLOSER: Tile = Tile {
        x: u16::MAX,
        y: u16::MAX,
        p0: TilePoint {
            x: u16::MAX,
            y: u16::MAX,
        },
        p1: TilePoint {
            x: u16::MAX,
            y: u16::MAX,
        },
    };
    for tile in tiles.iter().copied().chain([GATE_CLOSER]) {
        // Reset winding when going to the next line.
        //
        // TODO(Tom): I believe this is not necessary, unless tiles are culled.
        if first_tile.y != tile.y {
            winding_delta = 0;
        }

        // Push out the winding as an alpha mask when we move to the next location (i.e., a
        // tile without the same location).
        if first_tile.y != tile.y || prev_tile.x < tile.x {
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
        if first_tile.y != tile.y || prev_tile.x + 1 < tile.x {
            let strip = Strip {
                x: first_tile.x,
                y: first_tile.y,
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
        }

        prev_tile = tile;
        winding_delta += tile.delta();

        let p0_x = tile.p0.x as f32 * (Tile::WIDTH as f32 / u16::MAX as f32);
        let p0_y = tile.p0.y as f32 * (Tile::HEIGHT as f32 / u16::MAX as f32);
        let p1_x = tile.p1.x as f32 * (Tile::WIDTH as f32 / u16::MAX as f32);
        let p1_y = tile.p1.y as f32 * (Tile::HEIGHT as f32 / u16::MAX as f32);
        let x_slope = (p1_x - p0_x) / (p1_y - p0_y);
        let y_slope = (p1_y - p0_y) / (p1_x - p0_x);

        if p0_y == p1_y {
            continue;
        }

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
        for y_idx in 0..Tile::HEIGHT {
            let y = y_idx as f32;

            let sign = if p1_y <= p0_y { 1. } else { -1. };

            let y0 = p0_y.clamp(y, y + 1.);
            let y1 = p1_y.clamp(y, y + 1.);

            let ymin = f32::min(y0, y1);
            let ymax = f32::max(y0, y1);

            let mut acc = 0.;
            // TODO(Tom): reduce operations by taking the previous iteration's `y_right` as the
            // current iteration's `y_next`.
            // TODO(Tom): does short-circuiting help? e.g., if both x coordinates are to the
            // left of this pixel's right edge, breaking this inner loop?
            for x_idx in 0..Tile::WIDTH {
                let x = x_idx as f32;

                // Find the y-delta that happened within this pixel. Accumulate it forward.
                let y_left = (p0_y + (x - p0_x) * y_slope).clamp(ymin, ymax);
                let y_right = (p0_y + (x + 1. - p0_x) * y_slope).clamp(ymin, ymax);

                // Find the trapezoidal area within this pixel
                let y_left_x = p0_x + (y_left - p0_y) * x_slope;
                let y_right_x = p0_x + (y_right - p0_y) * x_slope;

                let h = (y_left - y_right).abs();
                let area = 0.5 * h * (x + x + 2. - y_left_x - y_right_x);
                location_winding[x_idx as usize][y_idx as usize] += acc + sign * area;
                acc += sign * h;
            }
            accumulated_winding[y_idx as usize] += acc;
        }
    }
}
