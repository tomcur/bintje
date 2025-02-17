//! An experimental renderer.

use kurbo::{flatten, PathEl};
use peniko::BrushRef;

mod line;
mod point;
mod strip;
mod tile;
mod wide_tile;

#[cfg(test)]
mod tests;

pub(crate) use line::Line;
pub(crate) use point::Point;
pub(crate) use strip::Strip;
pub(crate) use tile::Tile;
pub use wide_tile::{cpu_rasterize, WideTile};

/// The main render context.
pub struct Bintje {
    /// The width of the render target in pixels.
    width: u16,
    /// The height of the render target in pixels.
    height: u16,

    // TODO(Tom): actually implement clipping.
    #[expect(unused, reason = "TODO")]
    clip_stack: Vec<ClipState>,

    /// The rendered wide tiles.
    ///
    /// These contain the draw commands, from which rasterization can proceed.
    wide_tiles: Vec<WideTile>,
    /// Alpha masks
    alpha_masks: Vec<u8>,

    /// Reusable line scratch buffer.
    lines: Vec<Line>,
    /// Reusable tile scratch buffer.
    tiles: Vec<Tile>,
    /// Reusable strip scratch buffer.
    strips: Vec<Strip>,
}

/// Draw commands.
///
/// These consist of wide tiles to be rendered, each with a per-wide-tile command list. Draw
/// commands contain an index into the alpha mask buffer.
///
/// TODO(Tom): the name is confusing, as wide tiles also contain commands.
pub struct Commands<'c> {
    pub wide_tiles: &'c [WideTile],
    pub alpha_masks: &'c [u8],
}

#[derive(Debug)]
pub(crate) struct ClipState {
    // bounding_box: kurbo::Rect,
    // suppressed_wide_tiles: Vec<u16>,
}

impl Bintje {
    /// Create a new renderer with the given pixel width and height.
    pub fn new(width: u16, height: u16) -> Self {
        let wide_tile_columns = width.div_ceil(wide_tile::WIDE_TILE_WIDTH_PX);
        let wide_tile_rows = height.div_ceil(Tile::HEIGHT);

        let mut wide_tiles = Vec::new();
        for _ in 0..wide_tile_columns {
            for _ in 0..wide_tile_rows {
                wide_tiles.push(WideTile {
                    commands: Vec::new(),
                });
            }
        }

        Self {
            width,
            height,
            clip_stack: Vec::with_capacity(16),
            wide_tiles,
            alpha_masks: Vec::with_capacity(65536),
            lines: Vec::with_capacity(512),
            tiles: Vec::with_capacity(256),
            strips: Vec::with_capacity(64),
        }
    }

    /// The size of the current render context canvas in pixels.
    ///
    /// The size is returned as a tuple of `(width, height)`.
    pub fn size(&self) -> (u16, u16) {
        (self.width, self.height)
    }

    fn flatten_path(&mut self, path: impl kurbo::Shape) {
        let mut closed = true;
        let mut start = kurbo::Point::ZERO;
        let mut prev = kurbo::Point::ZERO;
        flatten(path.path_elements(0.01), 0.01, |path_element| {
            match path_element {
                PathEl::MoveTo(point) => {
                    if !closed {
                        self.lines
                            .push(Line::from_kurbo(kurbo::Line::new(prev, start)));
                        closed = true;
                    }
                    start = point;
                    prev = point;
                }
                PathEl::LineTo(point) => {
                    self.lines
                        .push(Line::from_kurbo(kurbo::Line::new(prev, point)));
                    prev = point;
                    closed = false;
                }
                PathEl::ClosePath => {
                    self.lines
                        .push(Line::from_kurbo(kurbo::Line::new(prev, start)));
                    closed = true;
                }
                // `flatten` turns the path into lines.
                PathEl::QuadTo(_, _) | PathEl::CurveTo(_, _, _) => unreachable!(),
            }
        });

        if !closed && prev != start {
            self.lines
                .push(Line::from_kurbo(kurbo::Line::new(prev, start)));
        }
    }

    /// Consume the lines, turning them into tiles.
    fn tile(&mut self) {
        for line in self.lines.drain(..) {
            tile::generate_tiles(line, |tile| {
                self.tiles.push(tile);
            });
        }
        self.tiles.sort_unstable();
    }

    /// Consume tiles, turning them into strips.
    fn strip(&mut self) {
        strip::generate_strips(&self.tiles, &mut self.alpha_masks, &mut self.strips);
    }

    /// Consume strips, turning them into wide tile commands.
    fn widen<'b>(&mut self, brush: impl Into<BrushRef<'b>>) {
        wide_tile::generate_wide_tile_commands(
            self.width,
            &mut self.wide_tiles,
            &self.strips,
            brush,
        );
    }

    /// Clear the scene and start again.
    pub fn clear(&mut self) {
        todo!()
    }

    /// Fill a shape defined by `path` with the given `brush` (currently only solid colors are
    /// supported).
    ///
    /// This generates wide tile draw commands.
    pub fn fill_shape<'b>(
        &mut self,
        path: impl kurbo::Shape,
        brush: impl Into<peniko::BrushRef<'b>>,
    ) {
        self.lines.clear();
        self.tiles.clear();
        self.strips.clear();
        self.flatten_path(path);
        self.tile();
        self.strip();
        self.widen(brush);
    }

    /// Stroke a shape defined by `path` with the given stroke style and `brush` (currently only
    /// solid colors are supported).
    ///
    /// This generates wide tile draw commands.
    pub fn stroke<'b>(
        &mut self,
        path: impl IntoIterator<Item = PathEl>,
        style: &kurbo::Stroke,
        brush: impl Into<peniko::BrushRef<'b>>,
    ) {
        self.lines.clear();
        self.tiles.clear();
        self.strips.clear();
        let lines: flatten::stroke::LoweredPath<kurbo::Line> =
            flatten::stroke::stroke_undashed(path, style, 0.25);
        for line in lines.path {
            self.lines.push(Line {
                p0: line.p0.into(),
                p1: line.p1.into(),
            });
        }
        self.strip();
        self.widen(brush);
    }

    /// Get the generated draw commands.
    pub fn commands(&self) -> Commands<'_> {
        Commands {
            wide_tiles: &self.wide_tiles,
            alpha_masks: &self.alpha_masks,
        }
    }
}
