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

        let y_top = f32::min(p0_y, p1_y);
        let y_bottom = f32::max(p0_y, p1_y);
        let x_left = f32::min(p0_x, p1_x);
        let x_right = f32::max(p0_x, p1_x);

        // The y-coordinate at the line's leftmost point.
        let x_left_y = if x_left == p0_x { p0_y } else { p1_y };

        let x_slope = (p1_x - p0_x) / (p1_y - p0_y);
        let y_slope = (p1_y - p0_y) / (p1_x - p0_x);

        let y_top_tiles = (y_top as u16).min(rows.len() as u16);
        let y_bottom_tiles = (y_bottom as u16).min(rows.len().saturating_sub(1) as u16);

        for y_idx in y_top_tiles..=y_bottom_tiles {
            let row = &mut rows[y_idx as usize];
            let row_y_top = (y_idx as f32).max(y_top).min(y_bottom);
            let row_y_bottom = ((y_idx + 1) as f32).max(y_top).min(y_bottom);

            if x_left < 0. {
                // Line's y-coord at the left viewport edge.
                let viewport_y_left = p0_y - y_slope * p0_x;
                row.winding +=
                    sign as i32 * (x_left_y < row_y_top && viewport_y_left >= row_y_top) as i32;
                for y_px in 0..Tile::HEIGHT {
                    // TODO(Tom): use constants
                    let px_y_top = row_y_top + (1. / Tile::HEIGHT as f32) * y_px as f32;
                    let px_y_bottom = row_y_top + (1. / Tile::HEIGHT as f32) * (y_px + 1) as f32;
                    row.area_coverage[y_px as usize] += Tile::HEIGHT as f32
                        * sign
                        * (x_left_y.max(px_y_top).min(px_y_bottom)
                            - viewport_y_left.max(px_y_top).min(px_y_bottom))
                        .abs();
                }
            }

            // The line's x-coordinates at the row's top- and bottom-most points.
            let row_y_top_x = p0_x + (row_y_top - p0_y) * x_slope;
            let row_y_bottom_x = p0_x + (row_y_bottom - p0_y) * x_slope;

            let row_left_x = f32::min(row_y_top_x, row_y_bottom_x).max(x_left);
            let row_right_x = f32::max(row_y_top_x, row_y_bottom_x).min(x_right);

            for x_idx in row_left_x as u16..(row_right_x as u16 + 1).min(width_in_tiles) {
                row.tiles.push(Tile { x: x_idx, line_idx });
            }
        }
    }
}
