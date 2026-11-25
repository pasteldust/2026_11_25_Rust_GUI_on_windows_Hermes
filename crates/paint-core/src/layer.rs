//! レイヤー（下絵は編集禁止）。

use crate::color::Rgba;
use crate::error::{PaintError, Result};
use crate::geometry::Point;
use crate::pixel_buffer::PixelBuffer;
use crate::tool::LineWidth;

/// レイヤーの識別子。ドキュメント内で一意。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct LayerId(pub u64);

impl std::fmt::Display for LayerId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "L{}", self.0)
    }
}

/// レイヤーの種類。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LayerKind {
    /// 下絵（読み込んだ画像）。**描画できない**。
    Background,
    /// 描画レイヤー。編集できる。
    Paint,
}

impl LayerKind {
    /// 保存形式で使うタグ。
    pub const fn tag(self) -> u8 {
        match self {
            Self::Background => 0,
            Self::Paint => 1,
        }
    }

    /// タグから戻す。
    pub const fn from_tag(tag: u8) -> Option<Self> {
        match tag {
            0 => Some(Self::Background),
            1 => Some(Self::Paint),
            _ => None,
        }
    }
}

/// 1 枚のレイヤー。
///
/// 画素を書き換えるメソッドは、[`LayerKind::Background`] の場合
/// [`PaintError::LayerLocked`] を返す。下絵が壊れないことを型の側で保証する。
#[derive(Debug, Clone, PartialEq)]
pub struct Layer {
    /// 識別子。
    pub id: LayerId,
    /// 表示名。
    pub name: String,
    /// 種類。
    pub kind: LayerKind,
    /// 表示するか。
    pub visible: bool,
    /// 不透明度（0.0〜1.0）。
    pub opacity: f32,
    /// 画素。
    pub pixels: PixelBuffer,
    /// このレイヤーで最後に使った線幅（保存対象）。
    pub line_width: LineWidth,
    /// このレイヤーで最後に使った色（保存対象）。
    pub color: Rgba,
}

impl Layer {
    /// 下絵レイヤーを作る。
    pub fn background(name: impl Into<String>, pixels: PixelBuffer) -> Self {
        Self {
            id: LayerId(0),
            name: name.into(),
            kind: LayerKind::Background,
            visible: true,
            opacity: 1.0,
            pixels,
            line_width: LineWidth::W1,
            color: Rgba::BLACK,
        }
    }

    /// 描画レイヤーを作る（全面透明）。
    pub fn paint(name: impl Into<String>, width: u32, height: u32) -> Self {
        Self {
            id: LayerId(0),
            name: name.into(),
            kind: LayerKind::Paint,
            visible: true,
            opacity: 1.0,
            pixels: PixelBuffer::new(width, height),
            line_width: LineWidth::W1,
            color: Rgba::BLACK,
        }
    }

    /// 編集できるか。
    pub const fn is_editable(&self) -> bool {
        matches!(self.kind, LayerKind::Paint)
    }

    /// 不透明度を 0〜1 に丸める。
    pub fn set_opacity(&mut self, opacity: f32) {
        self.opacity = opacity.clamp(0.0, 1.0);
    }

    /// 書き込み可能か確かめる。
    pub fn ensure_editable(&self) -> Result<()> {
        if self.is_editable() {
            Ok(())
        } else {
            Err(PaintError::LayerLocked {
                name: self.name.clone(),
            })
        }
    }

    /// 画素を置き換える。
    pub fn set_pixel(&mut self, p: Point, color: Rgba) -> Result<()> {
        self.ensure_editable()?;
        if self.pixels.set(p, color) {
            Ok(())
        } else {
            Err(PaintError::out_of_bounds(p))
        }
    }

    /// 画素を重ねる（source-over）。
    pub fn blend_pixel(&mut self, p: Point, color: Rgba) -> Result<()> {
        self.ensure_editable()?;
        if self.pixels.blend(p, color) {
            Ok(())
        } else {
            Err(PaintError::out_of_bounds(p))
        }
    }

    /// 画素を透明にする（消しゴム）。
    pub fn erase_pixel(&mut self, p: Point) -> Result<()> {
        self.set_pixel(p, Rgba::TRANSPARENT)
    }

    /// 全面を透明にする。
    pub fn clear(&mut self) -> Result<()> {
        self.ensure_editable()?;
        self.pixels.fill(Rgba::TRANSPARENT);
        Ok(())
    }

    /// このレイヤーの画素を `dst` の上に重ねる（表示・書き出し用）。
    pub fn blend_onto(&self, dst: &mut PixelBuffer) {
        if !self.visible || self.opacity <= 0.0 {
            return;
        }
        let opacity = self.opacity;
        let (w, h) = (
            dst.width().min(self.pixels.width()),
            dst.height().min(self.pixels.height()),
        );
        for y in 0..h {
            for x in 0..w {
                let p = Point::new(x as i32, y as i32);
                if let Some(src) = self.pixels.get(p) {
                    let src = if opacity >= 1.0 {
                        src
                    } else {
                        src.scaled_alpha(opacity)
                    };
                    if !src.is_transparent() {
                        dst.blend(p, src);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paint_layer_is_editable() {
        let mut l = Layer::paint("レイヤー 1", 4, 4);
        assert!(l.is_editable());
        l.set_pixel(Point::new(0, 0), Rgba::BLACK).unwrap();
        assert_eq!(l.pixels.get(Point::ORIGIN), Some(Rgba::BLACK));
        assert!(l.set_pixel(Point::new(9, 9), Rgba::BLACK).is_err());
        assert!(l.clear().is_ok());
        assert_eq!(l.pixels.get(Point::ORIGIN), Some(Rgba::TRANSPARENT));
    }

    #[test]
    fn background_layer_is_locked() {
        let mut l = Layer::background("下絵", PixelBuffer::filled(2, 2, Rgba::WHITE));
        assert!(!l.is_editable());
        assert!(matches!(
            l.set_pixel(Point::ORIGIN, Rgba::BLACK),
            Err(PaintError::LayerLocked { .. })
        ));
        assert!(l.blend_pixel(Point::ORIGIN, Rgba::BLACK).is_err());
        assert!(l.erase_pixel(Point::ORIGIN).is_err());
        assert!(l.clear().is_err());
        // 画素は無事
        assert_eq!(l.pixels.get(Point::ORIGIN), Some(Rgba::WHITE));
    }

    #[test]
    fn opacity_is_clamped() {
        let mut l = Layer::paint("a", 1, 1);
        l.set_opacity(3.0);
        assert_eq!(l.opacity, 1.0);
        l.set_opacity(-0.5);
        assert_eq!(l.opacity, 0.0);
    }

    #[test]
    fn blend_onto_skips_hidden() {
        let mut top = Layer::paint("top", 2, 2);
        top.set_pixel(Point::ORIGIN, Rgba::rgb(255, 0, 0)).unwrap();
        let mut dst = PixelBuffer::new(2, 2);
        top.blend_onto(&mut dst);
        assert_eq!(dst.get(Point::ORIGIN), Some(Rgba::rgb(255, 0, 0)));

        let mut hidden = Layer::paint("hidden", 2, 2);
        hidden.set_pixel(Point::ORIGIN, Rgba::WHITE).unwrap();
        hidden.visible = false;
        let mut dst2 = PixelBuffer::new(2, 2);
        hidden.blend_onto(&mut dst2);
        assert_eq!(dst2.get(Point::ORIGIN), Some(Rgba::TRANSPARENT));
    }

    #[test]
    fn kind_tag_round_trip() {
        for k in [LayerKind::Background, LayerKind::Paint] {
            assert_eq!(LayerKind::from_tag(k.tag()), Some(k));
        }
        assert_eq!(LayerKind::from_tag(9), None);
    }
}
