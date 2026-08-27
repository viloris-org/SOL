//! Rendering types (geometry, colors, elements).

/// 2D point in compositor space.
#[derive(Debug, Copy, Clone, PartialEq)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

impl Point {
    pub const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }

    pub const fn zero() -> Self {
        Self { x: 0.0, y: 0.0 }
    }
}

/// 2D size.
#[derive(Debug, Copy, Clone, PartialEq)]
pub struct Size {
    pub width: f64,
    pub height: f64,
}

impl Size {
    pub const fn new(width: f64, height: f64) -> Self {
        Self { width, height }
    }
}

/// Axis-aligned rectangle.
#[derive(Debug, Copy, Clone, PartialEq)]
pub struct Rectangle {
    pub loc: Point,
    pub size: Size,
}

impl Rectangle {
    pub const fn new(loc: Point, size: Size) -> Self {
        Self { loc, size }
    }

    pub fn contains(&self, point: Point) -> bool {
        point.x >= self.loc.x
            && point.x < self.loc.x + self.size.width
            && point.y >= self.loc.y
            && point.y < self.loc.y + self.size.height
    }
}

/// Transform applied to a surface (rotation, scale, flip).
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum Transform {
    Normal,
    Rotate90,
    Rotate180,
    Rotate270,
    Flipped,
    Flipped90,
    Flipped180,
    Flipped270,
}

/// RGBA color (0.0 - 1.0 range).
#[derive(Debug, Copy, Clone, PartialEq)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Color {
    pub const fn new(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self { r, g, b, a }
    }

    pub const fn rgb(r: f32, g: f32, b: f32) -> Self {
        Self::new(r, g, b, 1.0)
    }

    /// SOL default background (matches sol-design token).
    pub const BACKGROUND: Self = Self::new(0.11, 0.10, 0.13, 1.0);
}

/// Renderable element (surface, decoration, overlay).
#[derive(Debug, Clone)]
pub enum RenderElement {
    Surface {
        buffer_id: u32,
        location: Point,
        damage: Vec<Rectangle>,
        alpha: f32,
    },
    SolidRect {
        rect: Rectangle,
        color: Color,
    },
}
