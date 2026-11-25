//! 幾何（点・大きさ・矩形）。

use std::fmt;

/// 整数の点。キャンバスの画素座標は左上が原点。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Point {
    /// X 座標（右が正）。
    pub x: i32,
    /// Y 座標（下が正）。
    pub y: i32,
}

impl Point {
    /// 原点。
    pub const ORIGIN: Self = Self { x: 0, y: 0 };

    /// 作る。
    pub const fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }

    /// マンハッタン距離。
    pub fn manhattan(self, other: Self) -> i32 {
        (self.x - other.x).abs() + (self.y - other.y).abs()
    }

    /// 成分ごとの加算。
    pub const fn offset(self, dx: i32, dy: i32) -> Self {
        Self {
            x: self.x + dx,
            y: self.y + dy,
        }
    }
}

impl fmt::Display for Point {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "({}, {})", self.x, self.y)
    }
}

/// 大きさ（画素数）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Size {
    /// 幅。
    pub width: u32,
    /// 高さ。
    pub height: u32,
}

impl Size {
    /// 作る。
    pub const fn new(width: u32, height: u32) -> Self {
        Self { width, height }
    }

    /// 画素数。
    pub const fn pixel_count(self) -> u64 {
        self.width as u64 * self.height as u64
    }

    /// どちらかが 0。
    pub const fn is_empty(self) -> bool {
        self.width == 0 || self.height == 0
    }

    /// 保持に必要なバイト数（RGBA8）。
    pub const fn byte_len(self) -> usize {
        (self.pixel_count() * 4) as usize
    }
}

impl fmt::Display for Size {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}x{}", self.width, self.height)
    }
}

/// 整数の矩形（左上と大きさ）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Rect {
    /// 左上 X。
    pub x: i32,
    /// 左上 Y。
    pub y: i32,
    /// 幅。
    pub width: u32,
    /// 高さ。
    pub height: u32,
}

impl Rect {
    /// 作る。
    pub const fn new(x: i32, y: i32, width: u32, height: u32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    /// 2 点を囲む矩形（順序は不問）。
    pub fn from_points(a: Point, b: Point) -> Self {
        let x = a.x.min(b.x);
        let y = a.y.min(b.y);
        let w = (a.x.max(b.x) - x) as u32 + 1;
        let h = (a.y.max(b.y) - y) as u32 + 1;
        Self::new(x, y, w, h)
    }

    /// 大きさから作る（原点始まり）。
    pub const fn from_size(size: Size) -> Self {
        Self::new(0, 0, size.width, size.height)
    }

    /// 左上の点。
    pub const fn top_left(self) -> Point {
        Point::new(self.x, self.y)
    }

    /// 右下の点（内側の最後の画素）。
    pub const fn bottom_right(self) -> Point {
        Point::new(
            self.x + self.width as i32 - 1,
            self.y + self.height as i32 - 1,
        )
    }

    /// どちらかの辺が 0。
    pub const fn is_empty(self) -> bool {
        self.width == 0 || self.height == 0
    }

    /// 点を含むか。
    pub fn contains(self, p: Point) -> bool {
        !self.is_empty()
            && p.x >= self.x
            && p.y >= self.y
            && p.x < self.x + self.width as i32
            && p.y < self.y + self.height as i32
    }

    /// 共通部分。重なりが無ければ `None`。
    pub fn intersect(self, other: Self) -> Option<Self> {
        let x1 = self.x.max(other.x);
        let y1 = self.y.max(other.y);
        let x2 = (self.x + self.width as i32).min(other.x + other.width as i32);
        let y2 = (self.y + self.height as i32).min(other.y + other.height as i32);
        if x2 <= x1 || y2 <= y1 {
            None
        } else {
            Some(Self::new(x1, y1, (x2 - x1) as u32, (y2 - y1) as u32))
        }
    }

    /// 両方を囲む最小の矩形。
    pub fn union(self, other: Self) -> Self {
        if self.is_empty() {
            return other;
        }
        if other.is_empty() {
            return self;
        }
        let x = self.x.min(other.x);
        let y = self.y.min(other.y);
        let x2 = (self.x + self.width as i32).max(other.x + other.width as i32);
        let y2 = (self.y + self.height as i32).max(other.y + other.height as i32);
        Self::new(x, y, (x2 - x) as u32, (y2 - y) as u32)
    }

    /// 与えられた範囲に切り詰める。
    pub fn clamp_to(self, bounds: Self) -> Option<Self> {
        self.intersect(bounds)
    }
}

impl fmt::Display for Rect {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "({}, {}) {}x{}", self.x, self.y, self.width, self.height)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn point_math() {
        let a = Point::new(1, 2);
        assert_eq!(a.offset(3, -1), Point::new(4, 1));
        assert_eq!(Point::ORIGIN.manhattan(Point::new(3, 4)), 7);
        assert_eq!(a.to_string(), "(1, 2)");
    }

    #[test]
    fn size_helpers() {
        assert_eq!(Size::new(4, 5).pixel_count(), 20);
        assert_eq!(Size::new(4, 5).byte_len(), 80);
        assert!(Size::new(0, 5).is_empty());
        assert_eq!(Size::new(640, 480).to_string(), "640x480");
    }

    #[test]
    fn rect_from_points_normalizes() {
        let r = Rect::from_points(Point::new(10, 10), Point::new(4, 2));
        assert_eq!(r, Rect::new(4, 2, 7, 9));
        assert_eq!(r.bottom_right(), Point::new(10, 10));
    }

    #[test]
    fn rect_contains_edges() {
        let r = Rect::new(0, 0, 3, 3);
        assert!(r.contains(Point::new(0, 0)));
        assert!(r.contains(Point::new(2, 2)));
        assert!(!r.contains(Point::new(3, 3)));
        assert!(!r.contains(Point::new(-1, 0)));
        assert!(!Rect::new(0, 0, 0, 5).contains(Point::ORIGIN));
    }

    #[test]
    fn rect_intersect_and_union() {
        let a = Rect::new(0, 0, 10, 10);
        let b = Rect::new(5, 5, 10, 10);
        assert_eq!(a.intersect(b), Some(Rect::new(5, 5, 5, 5)));
        assert_eq!(a.union(b), Rect::new(0, 0, 15, 15));
        assert_eq!(a.intersect(Rect::new(20, 20, 1, 1)), None);
        assert_eq!(a.union(Rect::new(0, 0, 0, 0)), a);
    }

    #[test]
    fn rect_clamp() {
        let bounds = Rect::new(0, 0, 4, 4);
        assert_eq!(
            Rect::new(-2, -2, 4, 4).clamp_to(bounds),
            Some(Rect::new(0, 0, 2, 2))
        );
        assert_eq!(Rect::new(10, 10, 2, 2).clamp_to(bounds), None);
    }
}
