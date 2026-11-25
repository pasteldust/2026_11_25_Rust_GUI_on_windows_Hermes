//! ツール（点・直線・消しゴム・範囲消去・自由線）のラスタライズ。
//!
//! いずれの関数も [`paint_core::Layer`] へ直接描き込み、**影響した矩形**を返す。
//! 返した矩形は呼び出し側が [`crate::history::PixelPatch`] を作るのに使う
//! （変更のあった範囲だけを履歴へ積むため）。
//!
//! 下絵（[`paint_core::LayerKind::Background`]）へ描こうとすると
//! [`paint_core::PaintError::LayerLocked`] を返す。

use paint_core::{Layer, LineWidth, Point, Rect, Result, Rgba, ToolKind, ToolSettings};

/// 線幅の「半径」= 中心から何画素広がるか。
///
/// [`stamp_dot`] の円の広がりと [`line_bounds`] の膨らみで共通に使う。
fn half_extent(width: LineWidth) -> i32 {
    (width.pixels() / 2) as i32
}

/// 幅 0・高さ 0 の空矩形。
fn empty_rect() -> Rect {
    Rect::new(0, 0, 0, 0)
}

/// `rect` を `layer` の画素範囲へ切り詰める（完全に外側なら空矩形）。
fn clamp_to_layer(layer: &Layer, rect: Rect) -> Rect {
    rect.clamp_to(layer.pixels.bounds())
        .unwrap_or_else(empty_rect)
}

/// `at` を中心とする直径 `width.pixels()` の円の外接矩形。
fn dot_rect(at: Point, width: LineWidth) -> Rect {
    let half = half_extent(width);
    Rect::new(
        at.x - half,
        at.y - half,
        (2 * half + 1) as u32,
        (2 * half + 1) as u32,
    )
}

/// 2 点を結ぶ画素列（ブレゼンハム法、両端を含む）。
///
/// 隣り合う画素は必ず 8 近傍でつながる。
fn line_points(from: Point, to: Point) -> Vec<Point> {
    let dx = (to.x - from.x).abs();
    let dy = -(to.y - from.y).abs();
    let sx = if from.x < to.x { 1 } else { -1 };
    let sy = if from.y < to.y { 1 } else { -1 };
    let mut err = dx + dy;
    let (mut x, mut y) = (from.x, from.y);
    let mut out = Vec::new();
    loop {
        out.push(Point::new(x, y));
        if x == to.x && y == to.y {
            break;
        }
        let e2 = 2 * err;
        if e2 >= dy {
            err += dy;
            x += sx;
        }
        if e2 <= dx {
            err += dx;
            y += sy;
        }
    }
    out
}

/// 円を 1 つ描く。`replace` が `true` なら `set_pixel`（置換）、`false` なら
/// `blend_pixel`（重ね塗り）。範囲外の画素は静かに飛ばす。
fn stamp(layer: &mut Layer, at: Point, width: LineWidth, color: Rgba, replace: bool) -> Result<()> {
    let half = half_extent(width);
    let r = f64::from(width.pixels()) / 2.0;
    let r2 = r * r;
    for dy in -half..=half {
        for dx in -half..=half {
            if f64::from(dx * dx + dy * dy) <= r2 {
                let p = at.offset(dx, dy);
                if layer.pixels.contains(p) {
                    if replace {
                        layer.set_pixel(p, color)?;
                    } else {
                        layer.blend_pixel(p, color)?;
                    }
                }
            }
        }
    }
    Ok(())
}

/// `at` を中心に直径 `width.pixels()` の塗りつぶし円を描く（重ね塗り）。
///
/// `width = W1` のときは 1 画素だけを塗る。戻り値は影響した矩形を
/// `layer.pixels` の範囲へ切り詰めたもの。円が完全に外側なら幅 0・高さ 0 の
/// 矩形を返す（エラーにはしない）。
pub fn stamp_dot(layer: &mut Layer, at: Point, width: LineWidth, color: Rgba) -> Result<Rect> {
    layer.ensure_editable()?;
    stamp(layer, at, width, color, false)?;
    Ok(clamp_to_layer(layer, dot_rect(at, width)))
}

/// 太い直線を描く。ブレゼンハムで経路をたどり、各点へ [`stamp_dot`] する。
///
/// 戻り値は [`line_bounds`] を `layer.pixels` の範囲へ切り詰めたもの。
pub fn stroke_line(
    layer: &mut Layer,
    from: Point,
    to: Point,
    width: LineWidth,
    color: Rgba,
) -> Result<Rect> {
    layer.ensure_editable()?;
    for p in line_points(from, to) {
        stamp(layer, p, width, color, false)?;
    }
    Ok(clamp_to_layer(layer, line_bounds(from, to, width)))
}

/// 太い消しゴム線。経路は [`stroke_line`] と同じだが、`set_pixel` で
/// [`Rgba::TRANSPARENT`] に**置換**する（アルファだけでなく RGB も 0 になる）。
///
/// 戻り値は [`line_bounds`] を `layer.pixels` の範囲へ切り詰めたもの。
pub fn erase_line(layer: &mut Layer, from: Point, to: Point, width: LineWidth) -> Result<Rect> {
    layer.ensure_editable()?;
    for p in line_points(from, to) {
        stamp(layer, p, width, Rgba::TRANSPARENT, true)?;
    }
    Ok(clamp_to_layer(layer, line_bounds(from, to, width)))
}

/// 矩形の範囲を消す（[`paint_core::PixelBuffer::clear_rect`] による置換）。
///
/// 戻り値は実際に消した矩形を `layer.pixels` の範囲へ切り詰めたもの。
pub fn erase_rect(layer: &mut Layer, rect: Rect) -> Result<Rect> {
    layer.ensure_editable()?;
    layer.pixels.clear_rect(rect);
    Ok(clamp_to_layer(layer, rect))
}

/// 自由線を描く。連続する 2 点間を [`stroke_line`] でつなぐ。
///
/// 点が 1 つだけなら [`stamp_dot`]、空なら空矩形を返す。
pub fn draw_path(
    layer: &mut Layer,
    points: &[Point],
    width: LineWidth,
    color: Rgba,
) -> Result<Rect> {
    layer.ensure_editable()?;
    match points {
        [] => Ok(empty_rect()),
        [only] => stamp_dot(layer, *only, width, color),
        _ => {
            let mut acc = empty_rect();
            for seg in points.windows(2) {
                let r = stroke_line(layer, seg[0], seg[1], width, color)?;
                acc = acc.union(r);
            }
            Ok(acc)
        }
    }
}

/// [`stroke_line`] が触りうる矩形（2 点を囲む矩形を線幅の半径ぶん広げたもの）。
pub fn line_bounds(from: Point, to: Point, width: LineWidth) -> Rect {
    let base = Rect::from_points(from, to);
    let r = half_extent(width);
    Rect::new(
        base.x - r,
        base.y - r,
        base.width + (2 * r) as u32,
        base.height + (2 * r) as u32,
    )
}

/// 点列の外接矩形（線幅の半径ぶん広げる）。点が空なら `None`。
pub fn path_bounds(points: &[Point], width: LineWidth) -> Option<Rect> {
    let first = *points.first()?;
    let mut acc = Rect::from_points(first, first);
    for p in &points[1..] {
        acc = acc.union(Rect::from_points(*p, *p));
    }
    let r = half_extent(width);
    Some(Rect::new(
        acc.x - r,
        acc.y - r,
        acc.width + (2 * r) as u32,
        acc.height + (2 * r) as u32,
    ))
}

/// 自由線の点を貯めるビルダー。
///
/// ドラッグ入力のたびに [`StrokeBuilder::extend`] を呼ぶと、離れた 2 点の間が
/// 線形補間されて貯まるので、後で [`draw_path`] に渡しても線が途切れない。
#[derive(Debug, Clone)]
pub struct StrokeBuilder {
    settings: ToolSettings,
    start: Option<Point>,
    last: Option<Point>,
    points: Vec<Point>,
    active: bool,
}

impl StrokeBuilder {
    /// ツール設定から作る。
    pub fn new(settings: ToolSettings) -> Self {
        Self {
            settings,
            start: None,
            last: None,
            points: Vec::new(),
            active: false,
        }
    }

    /// ツールの種類。
    pub fn tool(&self) -> ToolKind {
        self.settings.tool
    }

    /// 描画を開始する（始点を記録し、点列を初期化する）。
    pub fn begin(&mut self, p: Point) {
        self.start = Some(p);
        self.last = Some(p);
        self.points.clear();
        self.points.push(p);
        self.active = true;
    }

    /// 点を追加する。
    ///
    /// 直前の点と同じなら無視する。離れていれば間を線形補間した点も貯める。
    /// まだ [`StrokeBuilder::begin`] していなければ、この点で開始する。
    pub fn extend(&mut self, p: Point) {
        if !self.active {
            self.begin(p);
            return;
        }
        if self.last == Some(p) {
            return;
        }
        if let Some(last) = self.last {
            for q in interpolate(last, p) {
                self.points.push(q);
            }
        }
        self.last = Some(p);
    }

    /// 描画を終え、貯まった点列を返す（内部の点列は空になる）。
    pub fn end(&mut self) -> Vec<Point> {
        self.active = false;
        std::mem::take(&mut self.points)
    }

    /// 貯まった点列。
    pub fn points(&self) -> &[Point] {
        &self.points
    }

    /// 始点。
    pub fn start_point(&self) -> Option<Point> {
        self.start
    }

    /// 直前に追加した点。
    pub fn last_point(&self) -> Option<Point> {
        self.last
    }

    /// 描画中か。
    pub fn is_active(&self) -> bool {
        self.active
    }
}

/// `from` から `to` へ 1 画素刻みで補間した点列（`to` を含み、`from` は含まない）。
fn interpolate(from: Point, to: Point) -> Vec<Point> {
    let dx = to.x - from.x;
    let dy = to.y - from.y;
    let steps = dx.abs().max(dy.abs());
    let mut out = Vec::new();
    if steps == 0 {
        return out;
    }
    for i in 1..=steps {
        out.push(Point::new(from.x + dx * i / steps, from.y + dy * i / steps));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use paint_core::{Document, LayerKind, PixelBuffer, Size};
    use std::collections::{HashSet, VecDeque};

    fn layer(w: u32, h: u32) -> Layer {
        Layer::paint("レイヤー", w, h)
    }

    /// 不透明な画素（描かれた画素）を集める。
    fn painted(layer: &Layer) -> Vec<Point> {
        let mut v = Vec::new();
        for y in 0..layer.pixels.height() as i32 {
            for x in 0..layer.pixels.width() as i32 {
                let p = Point::new(x, y);
                if layer.pixels.get(p).is_some_and(|c| !c.is_transparent()) {
                    v.push(p);
                }
            }
        }
        v
    }

    fn neighbors8(p: Point) -> [Point; 8] {
        [
            p.offset(-1, -1),
            p.offset(0, -1),
            p.offset(1, -1),
            p.offset(-1, 0),
            p.offset(1, 0),
            p.offset(-1, 1),
            p.offset(0, 1),
            p.offset(1, 1),
        ]
    }

    /// 8 近傍で連結している画素数（先頭の画素からたどれる数）。
    fn connected_count(pixels: &[Point]) -> usize {
        let set: HashSet<(i32, i32)> = pixels.iter().map(|p| (p.x, p.y)).collect();
        let Some(&start) = pixels.first() else {
            return 0;
        };
        let mut seen = HashSet::new();
        let mut q = VecDeque::new();
        seen.insert((start.x, start.y));
        q.push_back(start);
        while let Some(p) = q.pop_front() {
            for n in neighbors8(p) {
                if set.contains(&(n.x, n.y)) && seen.insert((n.x, n.y)) {
                    q.push_back(n);
                }
            }
        }
        seen.len()
    }

    #[test]
    fn stamp_dot_w1_is_single_pixel() {
        let mut l = layer(8, 8);
        let r = stamp_dot(&mut l, Point::new(4, 4), LineWidth::W1, Rgba::BLACK).unwrap();
        assert_eq!(painted(&l), vec![Point::new(4, 4)]);
        assert_eq!(r, Rect::new(4, 4, 1, 1));
    }

    #[test]
    fn stamp_dot_w8_is_roughly_circular() {
        let mut l = layer(32, 32);
        let r = stamp_dot(&mut l, Point::new(16, 16), LineWidth::W8, Rgba::BLACK).unwrap();
        let count = painted(&l).len();
        let expected = std::f64::consts::PI * 4.0 * 4.0; // π r², r = 4
        assert!(
            (count as f64 - expected).abs() < expected * 0.2,
            "塗られた画素数 {count} が πr²({expected:.1}) から離れすぎ"
        );
        assert!(l.pixels.get(Point::new(16, 16)).unwrap().a > 0);
        assert!(l.pixels.get(Point::new(20, 16)).unwrap().a > 0);
        assert_eq!(r, Rect::new(12, 12, 9, 9));
    }

    #[test]
    fn stamp_dot_fully_outside_does_not_error() {
        let mut l = layer(8, 8);
        let r = stamp_dot(&mut l, Point::new(-50, -50), LineWidth::W4, Rgba::BLACK).unwrap();
        assert!(r.is_empty());
        assert!(painted(&l).is_empty());
        let r2 = stamp_dot(&mut l, Point::new(0, 0), LineWidth::W8, Rgba::BLACK).unwrap();
        assert!(!r2.is_empty());
    }

    #[test]
    fn stroke_line_horizontal_is_connected() {
        let mut l = layer(20, 20);
        stroke_line(
            &mut l,
            Point::new(2, 5),
            Point::new(17, 5),
            LineWidth::W1,
            Rgba::BLACK,
        )
        .unwrap();
        let px = painted(&l);
        assert_eq!(px.len(), 16);
        assert!(l.pixels.get(Point::new(2, 5)).unwrap().a > 0);
        assert!(l.pixels.get(Point::new(17, 5)).unwrap().a > 0);
        assert_eq!(connected_count(&px), px.len());
    }

    #[test]
    fn stroke_line_vertical_is_connected() {
        let mut l = layer(20, 20);
        stroke_line(
            &mut l,
            Point::new(3, 1),
            Point::new(3, 18),
            LineWidth::W1,
            Rgba::BLACK,
        )
        .unwrap();
        let px = painted(&l);
        assert_eq!(px.len(), 18);
        assert_eq!(connected_count(&px), px.len());
    }

    #[test]
    fn stroke_line_diagonal_is_connected() {
        let mut l = layer(20, 20);
        stroke_line(
            &mut l,
            Point::new(1, 1),
            Point::new(18, 12),
            LineWidth::W1,
            Rgba::BLACK,
        )
        .unwrap();
        let px = painted(&l);
        assert!(l.pixels.get(Point::new(1, 1)).unwrap().a > 0);
        assert!(l.pixels.get(Point::new(18, 12)).unwrap().a > 0);
        assert_eq!(connected_count(&px), px.len());
    }

    #[test]
    fn thicker_line_covers_more_pixels() {
        let mut thin = layer(40, 40);
        let mut thick = layer(40, 40);
        stroke_line(
            &mut thin,
            Point::new(2, 20),
            Point::new(37, 20),
            LineWidth::W1,
            Rgba::BLACK,
        )
        .unwrap();
        stroke_line(
            &mut thick,
            Point::new(2, 20),
            Point::new(37, 20),
            LineWidth::W8,
            Rgba::BLACK,
        )
        .unwrap();
        assert!(painted(&thick).len() > painted(&thin).len());
    }

    #[test]
    fn erase_line_replaces_with_transparent() {
        let mut l = layer(16, 16);
        stroke_line(
            &mut l,
            Point::new(1, 8),
            Point::new(14, 8),
            LineWidth::W4,
            Rgba::rgb(200, 100, 50),
        )
        .unwrap();
        assert!(l.pixels.get(Point::new(8, 8)).unwrap().a > 0);
        erase_line(&mut l, Point::new(1, 8), Point::new(14, 8), LineWidth::W4).unwrap();
        assert_eq!(l.pixels.get(Point::new(8, 8)), Some(Rgba::TRANSPARENT));
        assert_eq!(painted(&l).len(), 0);
    }

    #[test]
    fn erase_rect_replaces_with_transparent() {
        let mut l = layer(8, 8);
        l.pixels.fill(Rgba::rgb(1, 2, 3));
        let r = erase_rect(&mut l, Rect::new(2, 2, 4, 4)).unwrap();
        assert_eq!(r, Rect::new(2, 2, 4, 4));
        for y in 2..6 {
            for x in 2..6 {
                assert_eq!(l.pixels.get(Point::new(x, y)), Some(Rgba::TRANSPARENT));
            }
        }
        assert_eq!(l.pixels.get(Point::new(0, 0)), Some(Rgba::rgb(1, 2, 3)));
    }

    #[test]
    fn background_layer_is_locked_for_all_tools() {
        let mut doc = Document::new(Size::new(8, 8));
        doc.add_background_layer("下絵", PixelBuffer::filled(8, 8, Rgba::WHITE))
            .unwrap();
        let bg = doc.layer_mut(0).unwrap();
        assert_eq!(bg.kind, LayerKind::Background);
        assert!(matches!(
            stamp_dot(bg, Point::new(1, 1), LineWidth::W1, Rgba::BLACK),
            Err(paint_core::PaintError::LayerLocked { .. })
        ));
        assert!(matches!(
            stroke_line(
                bg,
                Point::new(0, 0),
                Point::new(3, 3),
                LineWidth::W1,
                Rgba::BLACK
            ),
            Err(paint_core::PaintError::LayerLocked { .. })
        ));
        assert!(matches!(
            erase_line(bg, Point::new(0, 0), Point::new(3, 3), LineWidth::W1),
            Err(paint_core::PaintError::LayerLocked { .. })
        ));
        assert!(matches!(
            erase_rect(bg, Rect::new(0, 0, 2, 2)),
            Err(paint_core::PaintError::LayerLocked { .. })
        ));
        assert_eq!(bg.pixels.get(Point::new(1, 1)), Some(Rgba::WHITE));
    }

    #[test]
    fn line_crossing_canvas_edge_does_not_panic() {
        let mut l = layer(16, 16);
        let r = stroke_line(
            &mut l,
            Point::new(-10, 8),
            Point::new(30, 8),
            LineWidth::W2,
            Rgba::BLACK,
        )
        .unwrap();
        assert_eq!(r, Rect::new(0, 7, 16, 3));
        for p in painted(&l) {
            assert!(l.pixels.contains(p));
        }
        assert!(!painted(&l).is_empty());
    }

    #[test]
    fn draw_path_connects_segments() {
        let mut l = layer(20, 20);
        let pts = [Point::new(2, 2), Point::new(10, 2), Point::new(10, 10)];
        draw_path(&mut l, &pts, LineWidth::W1, Rgba::BLACK).unwrap();
        let px = painted(&l);
        assert!(l.pixels.get(Point::new(2, 2)).unwrap().a > 0);
        assert!(l.pixels.get(Point::new(10, 2)).unwrap().a > 0);
        assert!(l.pixels.get(Point::new(10, 10)).unwrap().a > 0);
        assert_eq!(connected_count(&px), px.len());
    }

    #[test]
    fn draw_path_single_point_is_dot() {
        let mut l = layer(8, 8);
        draw_path(&mut l, &[Point::new(4, 4)], LineWidth::W1, Rgba::BLACK).unwrap();
        assert_eq!(painted(&l), vec![Point::new(4, 4)]);
    }

    #[test]
    fn draw_path_empty_is_empty_rect() {
        let mut l = layer(8, 8);
        let r = draw_path(&mut l, &[], LineWidth::W1, Rgba::BLACK).unwrap();
        assert!(r.is_empty());
        assert!(painted(&l).is_empty());
    }

    #[test]
    fn line_bounds_expands_by_radius() {
        let r = line_bounds(Point::new(4, 4), Point::new(10, 4), LineWidth::W1);
        assert_eq!(r, Rect::new(4, 4, 7, 1));
        let r8 = line_bounds(Point::new(4, 4), Point::new(10, 4), LineWidth::W8);
        assert_eq!(r8, Rect::new(0, 0, 15, 9));
    }

    #[test]
    fn path_bounds_is_bounding_box() {
        assert_eq!(path_bounds(&[], LineWidth::W1), None);
        let pts = [Point::new(5, 5), Point::new(2, 9), Point::new(8, 1)];
        assert_eq!(
            path_bounds(&pts, LineWidth::W1),
            Some(Rect::new(2, 1, 7, 9))
        );
        assert_eq!(
            path_bounds(&pts, LineWidth::W4),
            Some(Rect::new(0, -1, 11, 13))
        );
    }

    #[test]
    fn builder_interpolates_and_ignores_duplicates() {
        let mut b = StrokeBuilder::new(ToolSettings::default());
        b.begin(Point::new(0, 0));
        b.extend(Point::new(10, 0));
        assert_eq!(b.points().len(), 11);
        assert_eq!(b.last_point(), Some(Point::new(10, 0)));
        b.extend(Point::new(10, 0));
        assert_eq!(b.points().len(), 11);
        b.extend(Point::new(10, 3));
        assert_eq!(b.points().len(), 14);
        assert_eq!(b.points()[11], Point::new(10, 1));
    }

    #[test]
    fn builder_lifecycle() {
        let mut b = StrokeBuilder::new(ToolSettings {
            tool: ToolKind::Eraser,
            ..ToolSettings::default()
        });
        assert_eq!(b.tool(), ToolKind::Eraser);
        assert!(!b.is_active());
        assert!(b.start_point().is_none());
        b.extend(Point::new(3, 3));
        assert!(b.is_active());
        assert_eq!(b.start_point(), Some(Point::new(3, 3)));
        let taken = b.end();
        assert_eq!(taken, vec![Point::new(3, 3)]);
        assert!(!b.is_active());
        assert!(b.points().is_empty());
    }

    #[test]
    fn line_points_single_when_equal() {
        assert_eq!(line_points(Point::new(5, 5), Point::new(5, 5)).len(), 1);
    }
}
