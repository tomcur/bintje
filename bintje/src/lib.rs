//! An experimental renderer.

use kurbo::{flatten, Affine, PathEl};
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
pub use tile::Tile;
pub use wide_tile::{cpu_rasterize, Command, Sample, SparseFill, WideTile};

/// The main render context.
pub struct Bintje {
    /// The width of the render target in pixels.
    width: u16,
    /// The height of the render target in pixels.
    height: u16,

    // TODO(Tom): actually implement clipping.
    #[expect(unused, reason = "TODO")]
    clip_stack: Vec<ClipState>,

    transform_stack: Vec<Transform>,
    current_transform: Affine,
    current_scale: f64,

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

    pub flattening_time: std::time::Duration,
    pub flattening_stroke_time: std::time::Duration,
    pub tile_generation_time: std::time::Duration,
    pub tile_sorting_time: std::time::Duration,
    pub strip_generation_time: std::time::Duration,
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

struct Transform {
    transform: Affine,
    scale: f64,
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
            transform_stack: Vec::with_capacity(16),
            current_transform: Affine::IDENTITY,
            current_scale: 1.,
            wide_tiles,
            alpha_masks: Vec::with_capacity(65536),
            lines: Vec::with_capacity(512),
            tiles: Vec::with_capacity(256),
            strips: Vec::with_capacity(64),

            flattening_time: std::time::Duration::ZERO,
            flattening_stroke_time: std::time::Duration::ZERO,
            tile_sorting_time: std::time::Duration::ZERO,
            tile_generation_time: std::time::Duration::ZERO,
            strip_generation_time: std::time::Duration::ZERO,
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
        let start_time = std::time::Instant::now();
        flatten(
            path.path_elements(0.25 / self.current_scale),
            0.25 / self.current_scale,
            |path_element| {
                let path_element = self.current_transform * path_element;
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
            },
        );

        if !closed && prev != start {
            self.lines
                .push(Line::from_kurbo(kurbo::Line::new(prev, start)));
        }
        self.flattening_time += start_time.elapsed();
    }

    /// Consume the lines, turning them into tiles.
    fn tile(&mut self) {
        let start = std::time::Instant::now();
        for line in self.lines.drain(..) {
            tile::generate_tiles(line, |tile| {
                self.tiles.push(tile);
            });
        }
        self.tile_generation_time += start.elapsed();
        let start = std::time::Instant::now();
        self.tiles.sort_unstable();
        self.tile_sorting_time += start.elapsed();
    }

    /// Consume tiles, turning them into strips.
    fn strip(&mut self) {
        let start = std::time::Instant::now();
        strip::generate_strips(&self.tiles, &mut self.alpha_masks, &mut self.strips);
        self.strip_generation_time += start.elapsed();
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
        for wide_tile in self.wide_tiles.iter_mut() {
            wide_tile.commands.clear();
        }
        self.transform_stack.clear();
        self.current_transform = Affine::IDENTITY;
        self.current_scale = 1.;
    }

    /// Push an affine transform. Subsequent commands will have this transform applied.
    ///
    /// The transform is combined with the previous transform.
    pub fn push_transform(&mut self, transform: Affine) {
        self.transform_stack.push(Transform {
            transform: self.current_transform,
            scale: self.current_scale,
        });

        self.current_transform *= transform;
        self.current_scale = f64::max(
            self.current_transform.as_coeffs()[0].abs(),
            self.current_transform.as_coeffs()[3].abs(),
        );
    }

    /// Pop the last-pushed affine transform, returning to the transform before it.
    pub fn pop_transform(&mut self) {
        if let Some(prev_transform) = self.transform_stack.pop() {
            self.current_transform = prev_transform.transform;
            self.current_scale = prev_transform.scale;
        }
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
        // Whether to use Kurbo's stroke expansion, or the experimental GPU stroke expansion
        // paper's expansion.
        const KURBO_STROKE_EXPANSION: bool = false;

        if KURBO_STROKE_EXPANSION {
            self.fill_shape(
                kurbo::stroke(
                    path,
                    style,
                    &kurbo::StrokeOpts::default(),
                    0.25 / self.current_scale,
                ),
                brush,
            );
        } else {
            self.lines.clear();
            self.tiles.clear();
            self.strips.clear();
            let start = std::time::Instant::now();
            let lines: flatten::stroke::LoweredPath<kurbo::Line> =
                flatten::stroke::stroke_undashed(path, style, 0.25 / self.current_scale);
            for line in lines.path {
                self.lines
                    .push(Line::from_kurbo(self.current_transform * line));
            }
            self.flattening_stroke_time += start.elapsed();
            self.tile();
            self.strip();
            self.widen(brush);
        }
    }

    /// Get the generated draw commands.
    pub fn commands(&self) -> Commands<'_> {
        Commands {
            wide_tiles: &self.wide_tiles,
            alpha_masks: &self.alpha_masks,
        }
    }
}
