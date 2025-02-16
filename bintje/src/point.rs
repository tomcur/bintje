#[derive(Clone, Copy, Debug)]
pub(crate) struct Point {
    pub x: f32,
    pub y: f32,
}

impl From<kurbo::Point> for Point {
    fn from(point: kurbo::Point) -> Self {
        Point {
            x: point.x as f32,
            y: point.y as f32,
        }
    }
}

impl Point {
    pub(crate) const fn new(x: f32, y: f32) -> Self {
        Point { x, y }
    }

    /// Rotate the point 90 degrees clockwise in a y-down coordinate system around the origin.
    pub(crate) const fn turn_90(self) -> Self {
        Self {
            x: -self.y,
            y: self.x,
        }
    }
}

impl std::ops::Add<Point> for Point {
    type Output = Self;

    fn add(self, rhs: Point) -> Self::Output {
        Point {
            x: self.x + rhs.x,
            y: self.y + rhs.y,
        }
    }
}

impl std::ops::Sub<Point> for Point {
    type Output = Self;

    fn sub(self, rhs: Point) -> Self::Output {
        Point {
            x: self.x - rhs.x,
            y: self.y - rhs.y,
        }
    }
}

impl std::ops::Mul<f32> for Point {
    type Output = Point;

    fn mul(self, rhs: f32) -> Self::Output {
        Point {
            x: rhs * self.x,
            y: rhs * self.y,
        }
    }
}
