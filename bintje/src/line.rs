use crate::Point;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Line {
    pub p0: Point,
    pub p1: Point,
}

impl Line {
    pub fn from_kurbo(line: kurbo::Line) -> Self {
        Self {
            p0: line.p0.into(),
            p1: line.p1.into(),
        }
    }

    /// Rotate the line 90 degrees clockwise in a y-down coordinate system around the origin.
    #[expect(unused, reason="may become useful again")]
    pub(crate) const fn turn_90(self) -> Line {
        Line {
            p0: self.p0.turn_90(),
            p1: self.p1.turn_90(),
        }
    }
}
