use crate::Line;

#[derive(Clone, Copy, Debug)]
pub struct Tile {
    /// The tile x-coordinate.
    pub(crate) x: u16,
    /// The index of the line that belongs to this tile into the line buffer.
    pub(crate) line_idx: u32,
}

impl Tile {
    /// Tile width in pixels.
    pub const WIDTH: u16 = 4;

    /// Tile height in pixels.
    pub const HEIGHT: u16 = 4;
}

impl std::cmp::PartialEq for Tile {
    fn eq(&self, other: &Self) -> bool {
        self.x == other.x
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
        self.x.cmp(&other.x)
    }
}

/// A row of tiles.
///
/// This accounts for the winding and pixel area coverage of geometry that lies to the left of the
/// row.
#[derive(Clone, Debug)]
pub(crate) struct TileRow {
    /// The tiles that make up this row.
    pub tiles: Vec<Tile>,
    /// The winding of the path at this tile row before the start of the row (i.e., the winding
    /// that occurred to the left of the viewport).
    pub winding: i32,
    /// The per-pixel area coverage of the path at this tile row before the start of the row (i.e.,
    /// the pixel coverage of the path segments to the left of the viewport).
    pub area_coverage: [f32; Tile::HEIGHT as usize],
}

impl TileRow {
    pub(crate) fn new() -> Self {
        TileRow {
            tiles: Vec::with_capacity(64),
            winding: 0,
            area_coverage: [0.; Tile::HEIGHT as usize],
        }
    }

    pub(crate) fn sort(&mut self) {
        self.tiles.sort_unstable();
    }

    pub(crate) fn clear(&mut self) {
        self.tiles.clear();
        self.winding = 0;
        self.area_coverage = [0.; Tile::HEIGHT as usize];
    }
}

pub(crate) fn generate_tiles(rows: &mut [TileRow], width: u16, lines: &[Line]) {
    for (line_idx, line) in lines.iter().copied().enumerate() {
        let line_idx = u32::try_from(line_idx).expect("Number of lines per path overflowed");

        let width_in_tiles = width.div_ceil(Tile::WIDTH);

        let p0_x = line.p0.x / Tile::WIDTH as f32;
        let p0_y = line.p0.y / Tile::HEIGHT as f32;
        let p1_x = line.p1.x / Tile::WIDTH as f32;
        let p1_y = line.p1.y / Tile::HEIGHT as f32;

        let sign = (p0_y - p1_y).signum();

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
        let y_top_tiles = (line_top_y as u16).min(rows.len().saturating_sub(1) as u16);
        let y_bottom_tiles = (line_bottom_y as u16).min(rows.len().saturating_sub(1) as u16);

        if line_left_x == line_right_x {
            if line_left_x < 0. {
                for y_idx in y_top_tiles..=y_bottom_tiles {
                    let row_top_y = y_idx as f32;
                    let row = &mut rows[y_idx as usize];
                    row.winding +=
                        sign as i32 * (line_top_y <= row_top_y && line_bottom_y > row_top_y) as i32;

                    for y_px in 0..Tile::HEIGHT {
                        let px_top_y = y_idx as f32 + y_px as f32 * (1. / Tile::HEIGHT as f32);
                        let px_bottom_y = y_idx as f32
                            + y_px as f32 * (1. / Tile::HEIGHT as f32)
                            + (1. / Tile::HEIGHT as f32);
                        row.area_coverage[y_px as usize] += Tile::HEIGHT as f32
                            * sign
                            * (line_bottom_y.min(px_bottom_y) - line_top_y.max(px_top_y)).max(0.);
                    }
                }
            } else {
                for y_idx in y_top_tiles..=y_bottom_tiles {
                    let x_idx = p0_x as u16;
                    let row = &mut rows[y_idx as usize];
                    row.tiles.push(Tile { x: x_idx, line_idx });
                }
            }
        } else {
            let x_slope = (p1_x - p0_x) / (p1_y - p0_y);
            if !x_slope.is_finite() {
                // TODO: elide horizontal lines.
                // unreachable!()
            }

            let y_top_tiles = (line_top_y as u16).min(rows.len() as u16);
            let y_bottom_tiles = (line_bottom_y as u16).min(rows.len().saturating_sub(1) as u16);

            for y_idx in y_top_tiles..=y_bottom_tiles {
                let row = &mut rows[y_idx as usize];
                let row_top_y = y_idx as f32;

                let ymin = line_top_y.max(row_top_y).min(row_top_y + 1.);
                let ymax = line_bottom_y.max(row_top_y).min(row_top_y + 1.);

                if line_left_x < 0. {
                    let y_slope = (line_right_y - line_left_y) / (line_right_x - line_left_x);
                    if !y_slope.is_finite() {
                        // Prevented by the branch above.
                        unreachable!()
                    }

                    // Line's y-coord at the left viewport edge.
                    let viewport_y_left = (line_left_y - line_left_x * y_slope)
                        .max(line_top_y)
                        .min(line_bottom_y);
                    row.winding += sign as i32
                        * ((line_left_y - row_top_y).signum()
                            != (viewport_y_left - row_top_y).signum())
                            as i32;
                    for y_px in 0..Tile::HEIGHT {
                        let px_top_y = y_idx as f32 + y_px as f32 * (1. / Tile::HEIGHT as f32);
                        let px_bottom_y = y_idx as f32
                            + y_px as f32 * (1. / Tile::HEIGHT as f32)
                            + (1. / Tile::HEIGHT as f32);
                        row.area_coverage[y_px as usize] += Tile::HEIGHT as f32
                            * sign
                            * (viewport_y_left.min(px_bottom_y).max(px_top_y)
                                - line_left_y.min(px_bottom_y).max(px_top_y))
                            .abs();
                    }
                }

                // The line's x-coordinates at the line's top- and bottom-most points within the
                // row.
                let row_y_top_x = p0_x + (ymin - p0_y) * x_slope;
                let row_y_bottom_x = p0_x + (ymax - p0_y) * x_slope;

                let row_left_x = f32::min(row_y_top_x, row_y_bottom_x).max(line_left_x);
                let row_right_x = f32::max(row_y_top_x, row_y_bottom_x).min(line_right_x);

                for x_idx in row_left_x as u16..(row_right_x as u16 + 1).min(width_in_tiles) {
                    row.tiles.push(Tile { x: x_idx, line_idx });
                }
            }
        }
    }
}
