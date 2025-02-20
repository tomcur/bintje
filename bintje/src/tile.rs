use crate::{Line, Point};

const ONE_MINUS_ULP: f32 = 0.99999994;
pub(crate) const ROBUST_EPSILON: f32 = 2e-7;

/// Point within a tile.
///
/// `(0,0)` is the top-left corner, and `(u16::MAX,u16::MAX)` is the point just shy of the
/// bottom-right corner. Note that points on edges of multiple tiles (like `(0,0)`) are considered
/// to be inside the last tile in scan order (i.e., the bottom-right-most tile).
#[derive(Clone, Copy, Debug)]
pub(crate) struct TilePoint {
    pub x: u16,
    pub y: u16,
}

impl TilePoint {
    fn from_point(point: Point) -> Self {
        Self {
            x: (point.x * (u16::MAX / Tile::WIDTH) as f32).round() as u16,
            y: (point.y * (u16::MAX / Tile::HEIGHT) as f32).round() as u16,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Tile {
    /// The tile x-coordinate.
    pub(crate) x: u16,
    /// The tile y-coordinate.
    pub(crate) y: u16,
    /// First point of the line within the tile, packed as two 16-bit floats.
    pub(crate) p0: TilePoint,
    /// Second point of the line within the tile, packed as two 16-bit floats.
    pub(crate) p1: TilePoint,
}

impl Tile {
    /// Tile width in pixels.
    pub const WIDTH: u16 = 4;

    /// Tile height in pixels.
    pub const HEIGHT: u16 = 4;
}

impl std::cmp::PartialEq for Tile {
    fn eq(&self, other: &Self) -> bool {
        (self.y, self.x) == (other.y, other.x)
    }
}

impl std::cmp::Eq for Tile {}

impl std::cmp::PartialOrd for Tile {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl std::cmp::Ord for Tile {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        (self.y, self.x).cmp(&(other.y, other.x))
    }
}

fn span(a: f32, b: f32) -> u32 {
    (a.max(b).ceil() - a.min(b).floor()).max(1.0) as u32
}

impl Tile {
    /// The tile's line path "delta", used for calculating path winding numbers.
    ///
    /// This is either `-1`, `0` or `1`. It is nonzero when the line in this tile crosses the top
    /// edge. It is `-1` when the line crosses the tile's top edge moving downards, and `1` the
    /// line crosses the tile's top edge moving upwards.
    pub(crate) fn delta(&self) -> i32 {
        (self.p1.y == 0) as i32 - (self.p0.y == 0) as i32
    }
}

pub(crate) fn generate_tiles(line: Line, mut callback: impl FnMut(Tile)) {
    const TILE_SCALE_X: f32 = 1.0 / Tile::WIDTH as f32;
    const TILE_SCALE_Y: f32 = 1.0 / Tile::HEIGHT as f32;

    // This is adapted from
    // https://github.com/googlefonts/compute-shader-101/blob/f304096319f65c48f64840fddb9be6a539b20741/compute-shader-toy/src/tiling.rs#L96-L234
    let p0 = line.p0;
    let p1 = line.p1;
    let is_down = p1.y >= p0.y;
    let (orig_xy0, orig_xy1) = if is_down { (p0, p1) } else { (p1, p0) };
    let s0 = orig_xy0 * TILE_SCALE_X;
    let s1 = orig_xy1 * TILE_SCALE_Y;

    // The number of horizontal tiles spanned by the line.
    let count_x = span(s0.x, s1.x) - 1;
    let count = count_x + span(s0.y, s1.y);

    let dx = (s1.x - s0.x).abs();
    let dy = s1.y - s0.y;
    if dx + dy == 0.0 {
        return;
    }
    if dy == 0.0 && s0.y.floor() == s0.y {
        return;
    }
    let idxdy = 1.0 / (dx + dy);
    let mut a = dx * idxdy;
    let is_positive_slope = s1.x >= s0.x;
    let sign = if is_positive_slope { 1.0 } else { -1.0 };
    let xt0 = (s0.x * sign).floor();
    let c = s0.x * sign - xt0;
    let y0 = s0.y.floor();
    let ytop = if s0.y == s1.y { s0.y.ceil() } else { y0 + 1.0 };
    let b = ((dy * c + dx * (ytop - s0.y)) * idxdy).min(ONE_MINUS_ULP);
    let robust_err = (a * (count as f32 - 1.0) + b).floor() - count_x as f32;
    if robust_err != 0.0 {
        a -= ROBUST_EPSILON.copysign(robust_err);
    }
    let x0 = xt0 * sign + if is_positive_slope { 0.0 } else { -1.0 };

    let imin = 0;
    let imax = count;
    // In the Vello source, here's where we do clipping to viewport (by setting
    // imin and imax to more restrictive values).
    // Note: we don't really need to compute this if imin == 0, but it's cheap
    let mut last_z = (a * (imin as f32 - 1.0) + b).floor();
    for i in imin..imax {
        let zf = a * i as f32 + b;
        let z = zf.floor();
        let y = (y0 + i as f32 - z) as i32;
        let x = (x0 + sign * z) as i32;

        let tile_xy = Point::new(
            x as f32 * Tile::WIDTH as f32,
            y as f32 * Tile::HEIGHT as f32,
        );
        let tile_xy1 = tile_xy + Point::new(Tile::WIDTH as f32, Tile::HEIGHT as f32);

        let mut xy0 = orig_xy0;
        let mut xy1 = orig_xy1;
        if i > 0 {
            if z == last_z {
                // Top edge is clipped
                // This calculation should arguably be done on orig_xy. Also might
                // be worth retaining slope.
                let mut xt = xy0.x + (xy1.x - xy0.x) * (tile_xy.y - xy0.y) / (xy1.y - xy0.y);
                xt = xt.clamp(tile_xy.x + 1e-3, tile_xy1.x);
                xy0 = Point::new(xt, tile_xy.y);
            } else {
                // If is_positive_slope, left edge is clipped, otherwise right
                let x_clip = if is_positive_slope {
                    tile_xy.x
                } else {
                    tile_xy1.x
                };
                let mut yt = xy0.y + (xy1.y - xy0.y) * (x_clip - xy0.x) / (xy1.x - xy0.x);
                yt = yt.clamp(tile_xy.y + 1e-3, tile_xy1.y);
                xy0 = Point::new(x_clip, yt);
            }
        }
        if i < count - 1 {
            let z_next = (a * (i as f32 + 1.0) + b).floor();
            if z == z_next {
                // Bottom edge is clipped
                let mut xt = xy0.x + (xy1.x - xy0.x) * (tile_xy1.y - xy0.y) / (xy1.y - xy0.y);
                xt = xt.clamp(tile_xy.x + 1e-3, tile_xy1.x);
                xy1 = Point::new(xt, tile_xy1.y);
            } else {
                // If is_positive_slope, right edge is clipped, otherwise left
                let x_clip = if is_positive_slope {
                    tile_xy1.x
                } else {
                    tile_xy.x
                };
                let mut yt = xy0.y + (xy1.y - xy0.y) * (x_clip - xy0.x) / (xy1.x - xy0.x);
                yt = yt.clamp(tile_xy.y + 1e-3, tile_xy1.y);
                xy1 = Point::new(x_clip, yt);
            }
        }
        // Apply numerical robustness logic
        let mut p0 = xy0 - tile_xy;
        let mut p1 = xy1 - tile_xy;
        // one count in fixed point
        const EPSILON: f32 = 1.0 / 8192.0;
        if p0.x < EPSILON {
            if p1.x < EPSILON {
                p0.x = EPSILON;
                if p0.y < EPSILON {
                    // Entire tile
                    p1.x = EPSILON;
                    p1.y = Tile::HEIGHT as f32;
                } else {
                    // Make segment disappear
                    p1.x = 2.0 * EPSILON;
                    p1.y = p0.y;
                }
            } else if p0.y < EPSILON {
                p0.x = EPSILON;
            }
        } else if p1.x < EPSILON && p1.y < EPSILON {
            p1.x = EPSILON;
        }
        // Question: do we need these? Also, maybe should be post-rounding?
        if p0.x == p0.x.floor() && p0.x != 0.0 {
            p0.x -= EPSILON;
        }
        if p1.x == p1.x.floor() && p1.x != 0.0 {
            p1.x -= EPSILON;
        }
        if !is_down {
            (p0, p1) = (p1, p0);
        }
        // These are regular asserts in Vello, but are debug asserts
        // here for performance reasons.
        debug_assert!(p0.x >= 0.0 && p0.x <= Tile::WIDTH as f32);
        debug_assert!(p0.y >= 0.0 && p0.y <= Tile::HEIGHT as f32);
        debug_assert!(p1.x >= 0.0 && p1.x <= Tile::WIDTH as f32);
        debug_assert!(p1.y >= 0.0 && p1.y <= Tile::HEIGHT as f32);
        let tile = Tile {
            // The tiles are shifted to the right here, to ensure geometry that is to the left of
            // the viewport can be accounted for in winding calculations.
            x: (x + 1).clamp(0, u16::MAX as i32) as u16,
            y: y.clamp(0, u16::MAX as i32) as u16,
            p0: TilePoint::from_point(p0),
            p1: TilePoint::from_point(p1),
        };
        callback(tile);

        last_z = z;
    }
}
