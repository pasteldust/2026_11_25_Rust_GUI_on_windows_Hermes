//! 画素配列（RGBA8・ストレートアルファ）。

use crate::color::Rgba;
use crate::error::{PaintError, Result};
use crate::geometry::{Point, Rect, Size};

/// 扱える 1 辺の最大画素数。
pub const MAX_DIMENSION: u32 = 16384;

/// RGBA8 の画素配列。
///
/// 範囲外へのアクセスは panic せず `None` / `false` を返す方針
/// （描画中はキャンバスの外を指すことが普通に起きるため）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PixelBuffer {
    width: u32,
    height: u32,
    data: Vec<u8>,
}

impl PixelBuffer {
    /// 全画素が透明なバッファを作る。
    ///
    /// # Panics
    /// 呼び出し側の指定ミスを早期に見つけるため、`width` / `height` が 0 か
    /// [`MAX_DIMENSION`] を超える場合は panic する。利用者入力に由来する値は
    /// 先に [`Size::is_empty`] などで検証すること。
    pub fn new(width: u32, height: u32) -> Self {
        Self::filled(width, height, Rgba::TRANSPARENT)
    }

    /// 単色で塗りつぶしたバッファを作る。
    pub fn filled(width: u32, height: u32, color: Rgba) -> Self {
        if width == 0 || height == 0 || width > MAX_DIMENSION || height > MAX_DIMENSION {
            panic!("不正な大きさ: {width}x{height}（1〜{MAX_DIMENSION} の範囲）");
        }
        let mut data = vec![0u8; (width as usize) * (height as usize) * 4];
        if !color.is_transparent() {
            for px in data.chunks_exact_mut(4) {
                px.copy_from_slice(&color.to_array());
            }
        }
        Self {
            width,
            height,
            data,
        }
    }

    /// 既存の RGBA8 データから作る（長さが合わなければエラー）。
    pub fn from_rgba8(width: u32, height: u32, data: Vec<u8>) -> Result<Self> {
        if width == 0 || height == 0 || width > MAX_DIMENSION || height > MAX_DIMENSION {
            return Err(PaintError::InvalidSize {
                width,
                height,
                max: MAX_DIMENSION,
            });
        }
        let expected = (width as usize) * (height as usize) * 4;
        if data.len() != expected {
            return Err(PaintError::InvalidDataLength {
                expected,
                actual: data.len(),
            });
        }
        Ok(Self {
            width,
            height,
            data,
        })
    }

    /// 幅。
    pub const fn width(&self) -> u32 {
        self.width
    }

    /// 高さ。
    pub const fn height(&self) -> u32 {
        self.height
    }

    /// 大きさ。
    pub const fn size(&self) -> Size {
        Size::new(self.width, self.height)
    }

    /// 画素数。
    pub fn pixel_count(&self) -> usize {
        (self.width as usize) * (self.height as usize)
    }

    /// 生のバイト列。
    pub fn data(&self) -> &[u8] {
        &self.data
    }

    /// 生のバイト列（書き換え用）。
    ///
    /// 長さと並び（RGBA8 行優先）を壊さないこと。壊れた場合は [`Self::from_rgba8`] の
    /// 検証を通らないため、外部から読み込んだデータには [`Self::from_rgba8`] を使う。
    pub fn data_mut(&mut self) -> &mut [u8] {
        &mut self.data
    }

    /// バッファ全体を覆う矩形。
    pub const fn bounds(&self) -> Rect {
        Rect::new(0, 0, self.width, self.height)
    }

    /// 座標を含むか。
    pub fn contains(&self, p: Point) -> bool {
        p.x >= 0 && p.y >= 0 && (p.x as u32) < self.width && (p.y as u32) < self.height
    }

    /// 座標のバイト位置。
    pub fn index_of(&self, p: Point) -> Option<usize> {
        if self.contains(p) {
            Some(((p.y as usize) * (self.width as usize) + (p.x as usize)) * 4)
        } else {
            None
        }
    }

    /// 画素を読む。
    pub fn get(&self, p: Point) -> Option<Rgba> {
        let i = self.index_of(p)?;
        Some(Rgba::from_array([
            self.data[i],
            self.data[i + 1],
            self.data[i + 2],
            self.data[i + 3],
        ]))
    }

    /// 画素を書く（置き換え）。範囲外なら `false`。
    pub fn set(&mut self, p: Point, color: Rgba) -> bool {
        match self.index_of(p) {
            Some(i) => {
                self.data[i..i + 4].copy_from_slice(&color.to_array());
                true
            }
            None => false,
        }
    }

    /// 画素を重ねる（source-over）。範囲外なら `false`。
    pub fn blend(&mut self, p: Point, color: Rgba) -> bool {
        match self.index_of(p) {
            Some(i) => {
                let dst = Rgba::from_array([
                    self.data[i],
                    self.data[i + 1],
                    self.data[i + 2],
                    self.data[i + 3],
                ]);
                self.data[i..i + 4].copy_from_slice(&color.blend_over(dst).to_array());
                true
            }
            None => false,
        }
    }

    /// すべての画素を置き換える。
    pub fn fill(&mut self, color: Rgba) {
        for px in self.data.chunks_exact_mut(4) {
            px.copy_from_slice(&color.to_array());
        }
    }

    /// 矩形を単色で置き換える（はみ出しは切り取る）。
    pub fn fill_rect(&mut self, rect: Rect, color: Rgba) {
        let Some(r) = rect.clamp_to(self.bounds()) else {
            return;
        };
        let bytes = color.to_array();
        for y in r.y..(r.y + r.height as i32) {
            for x in r.x..(r.x + r.width as i32) {
                if let Some(i) = self.index_of(Point::new(x, y)) {
                    self.data[i..i + 4].copy_from_slice(&bytes);
                }
            }
        }
    }

    /// 矩形の画素をすべて透明にする（消しゴムの範囲消去）。
    pub fn clear_rect(&mut self, rect: Rect) {
        self.fill_rect(rect, Rgba::TRANSPARENT);
    }

    /// 矩形を切り出す（はみ出しは切り取る）。
    pub fn crop(&self, rect: Rect) -> Option<Self> {
        let r = rect.clamp_to(self.bounds())?;
        let mut out = Self::new(r.width, r.height);
        for y in 0..r.height as i32 {
            for x in 0..r.width as i32 {
                if let Some(c) = self.get(Point::new(r.x + x, r.y + y)) {
                    out.set(Point::new(x, y), c);
                }
            }
        }
        Some(out)
    }

    /// 矩形部分の画素を差し替える（`rect` はこのバッファ座標）。
    pub fn blit(&mut self, rect: Rect, src: &Self, src_origin: Point) {
        for y in 0..rect.height as i32 {
            for x in 0..rect.width as i32 {
                let dst_p = Point::new(rect.x + x, rect.y + y);
                let src_p = Point::new(src_origin.x + x, src_origin.y + y);
                if let Some(c) = src.get(src_p) {
                    self.set(dst_p, c);
                }
            }
        }
    }

    /// 画素数が一致するか。
    pub fn same_size(&self, other: &Self) -> bool {
        self.width == other.width && self.height == other.height
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_is_transparent() {
        let b = PixelBuffer::new(4, 3);
        assert_eq!(b.size(), Size::new(4, 3));
        assert_eq!(b.pixel_count(), 12);
        assert!(b.data().chunks_exact(4).all(|p| p[3] == 0));
        assert_eq!(b.get(Point::new(3, 2)), Some(Rgba::TRANSPARENT));
    }

    #[test]
    fn out_of_bounds_is_soft() {
        let mut b = PixelBuffer::new(2, 2);
        assert_eq!(b.get(Point::new(2, 0)), None);
        assert_eq!(b.get(Point::new(-1, 0)), None);
        assert!(!b.set(Point::new(0, -1), Rgba::BLACK));
        assert!(!b.blend(Point::new(5, 5), Rgba::WHITE));
        assert!(!b.contains(Point::new(1, 2)));
    }

    #[test]
    fn set_and_blend() {
        let mut b = PixelBuffer::new(1, 1);
        assert!(b.set(Point::ORIGIN, Rgba::WHITE));
        assert_eq!(b.get(Point::ORIGIN), Some(Rgba::WHITE));
        let half = Rgba::new(0, 0, 0, 128);
        assert!(b.blend(Point::ORIGIN, half));
        let out = b.get(Point::ORIGIN).unwrap();
        assert!(out.r < 200 && out.a == 255, "実際: {out:?}");
    }

    #[test]
    fn fill_rect_clips() {
        let mut b = PixelBuffer::new(4, 4);
        b.fill_rect(Rect::new(-2, -2, 4, 4), Rgba::BLACK);
        assert_eq!(b.get(Point::new(0, 0)), Some(Rgba::BLACK));
        assert_eq!(b.get(Point::new(1, 1)), Some(Rgba::BLACK));
        assert_eq!(b.get(Point::new(2, 2)), Some(Rgba::TRANSPARENT));
        // 完全に外側でも panic しない
        b.fill_rect(Rect::new(10, 10, 2, 2), Rgba::WHITE);
    }

    #[test]
    fn clear_and_crop_and_blit() {
        let mut b = PixelBuffer::filled(4, 4, Rgba::WHITE);
        b.clear_rect(Rect::new(1, 1, 2, 2));
        assert_eq!(b.get(Point::new(1, 1)), Some(Rgba::TRANSPARENT));

        let c = b.crop(Rect::new(1, 1, 2, 2)).unwrap();
        assert_eq!(c.size(), Size::new(2, 2));

        let mut d = PixelBuffer::new(4, 4);
        d.blit(Rect::new(0, 0, 2, 2), &c, Point::ORIGIN);
        assert_eq!(d.get(Point::new(0, 0)), Some(Rgba::TRANSPARENT));
    }

    #[test]
    fn rejects_bad_sizes_and_lengths() {
        assert!(PixelBuffer::from_rgba8(2, 2, vec![0; 15]).is_err());
        assert!(PixelBuffer::from_rgba8(2, 2, vec![0; 16]).is_ok());
        assert!(PixelBuffer::from_rgba8(0, 2, vec![]).is_err());
        assert!(PixelBuffer::from_rgba8(MAX_DIMENSION + 1, 1, vec![0; 4]).is_err());
    }

    #[test]
    #[should_panic(expected = "不正な大きさ")]
    fn zero_size_panics() {
        let _ = PixelBuffer::new(0, 10);
    }
}
