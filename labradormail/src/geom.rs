//! Basic geometry types for window layout.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Point {
    pub col: i16,
    pub row: i16,
}

impl Point {
    pub const fn new(col: i16, row: i16) -> Self {
        Self { col, row }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Size {
    pub cols: i16,
    pub rows: i16,
}

impl Size {
    pub const fn new(cols: i16, rows: i16) -> Self {
        Self { cols, rows }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Rect {
    pub origin: Point,
    pub size: Size,
}

impl Rect {
    pub const fn new(origin: Point, size: Size) -> Self {
        Self { origin, size }
    }
}
