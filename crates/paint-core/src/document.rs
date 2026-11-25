//! ドキュメント（キャンバスとレイヤー群）。

use crate::color::Rgba;
use crate::error::{PaintError, Result};
use crate::geometry::{Rect, Size};
use crate::layer::{Layer, LayerId, LayerKind};
use crate::pixel_buffer::{PixelBuffer, MAX_DIMENSION};

/// キャンバスとレイヤーをまとめた文書。
///
/// レイヤーの並びは「先頭が最背面」。描画は先頭から順に重ねる。
#[derive(Debug, Clone, PartialEq)]
pub struct Document {
    canvas: Size,
    layers: Vec<Layer>,
    active: usize,
    next_layer_id: u64,
}

impl Document {
    /// 描画レイヤー 1 枚だけを持つ空の文書を作る。
    pub fn new(canvas: Size) -> Self {
        assert!(
            !canvas.is_empty() && canvas.width <= MAX_DIMENSION && canvas.height <= MAX_DIMENSION,
            "不正なキャンバス: {canvas}"
        );
        let mut doc = Self {
            canvas,
            layers: Vec::new(),
            active: 0,
            next_layer_id: 1,
        };
        doc.add_paint_layer("レイヤー 1");
        doc
    }

    /// 読み込んだレイヤー群から文書を組み立てる（保存ファイルの読み込み用）。
    ///
    /// 描画レイヤーが 1 枚も無い場合は透明な 1 枚を足す。ID の重複は付け替えて解消する。
    pub fn from_parts(canvas: Size, mut layers: Vec<Layer>, active: usize) -> Result<Self> {
        if canvas.is_empty() || canvas.width > MAX_DIMENSION || canvas.height > MAX_DIMENSION {
            return Err(PaintError::InvalidSize {
                width: canvas.width,
                height: canvas.height,
                max: MAX_DIMENSION,
            });
        }
        for layer in &layers {
            if layer.pixels.size() != canvas {
                return Err(PaintError::SizeMismatch {
                    given: layer.pixels.size(),
                    expected: canvas,
                });
            }
        }

        // ID を振り直す（欠番・重複を許さない）
        let mut next = 1u64;
        for layer in &mut layers {
            layer.id = LayerId(next);
            next += 1;
        }

        if !layers.iter().any(|l| l.kind == LayerKind::Paint) {
            let mut layer = Layer::paint("レイヤー 1", canvas.width, canvas.height);
            layer.id = LayerId(next);
            next += 1;
            layers.push(layer);
        }

        let active = active.min(layers.len() - 1);
        Ok(Self {
            canvas,
            layers,
            active,
            next_layer_id: next,
        })
    }

    /// キャンバスの大きさ。
    pub const fn canvas_size(&self) -> Size {
        self.canvas
    }

    /// レイヤー一覧（最背面が先頭）。
    pub fn layers(&self) -> &[Layer] {
        &self.layers
    }

    /// レイヤー数。
    pub fn layer_count(&self) -> usize {
        self.layers.len()
    }

    /// 選択中のレイヤーのインデックス。
    pub const fn active_index(&self) -> usize {
        self.active
    }

    /// 選択中のレイヤー。
    pub fn active_layer(&self) -> Option<&Layer> {
        self.layers.get(self.active)
    }

    /// 選択中のレイヤー（可変）。
    pub fn active_layer_mut(&mut self) -> Option<&mut Layer> {
        self.layers.get_mut(self.active)
    }

    /// 選択中のレイヤーを切り替える。
    pub fn set_active_index(&mut self, index: usize) -> bool {
        if index < self.layers.len() {
            self.active = index;
            true
        } else {
            false
        }
    }

    /// インデックス指定でレイヤーを得る。
    pub fn layer(&self, index: usize) -> Option<&Layer> {
        self.layers.get(index)
    }

    /// インデックス指定でレイヤーを得る（可変。画素の書き込みは下絵では失敗する）。
    pub fn layer_mut(&mut self, index: usize) -> Option<&mut Layer> {
        self.layers.get_mut(index)
    }

    /// ID からレイヤーを探す。
    pub fn layer_by_id(&self, id: LayerId) -> Option<&Layer> {
        self.layers.iter().find(|l| l.id == id)
    }

    /// 新しい ID を払い出す。
    fn issue_id(&mut self) -> LayerId {
        let id = LayerId(self.next_layer_id);
        self.next_layer_id += 1;
        id
    }

    /// 描画レイヤーを追加し、そのインデックスを返す（選択も移す）。
    pub fn add_paint_layer(&mut self, name: impl Into<String>) -> usize {
        let id = self.issue_id();
        let mut layer = Layer::paint(name, self.canvas.width, self.canvas.height);
        layer.id = id;
        self.layers.push(layer);
        self.active = self.layers.len() - 1;
        self.active
    }

    /// 下絵レイヤーを最背面に追加する（読み込んだ画像を載せる）。
    ///
    /// 大きさがキャンバスと一致しない画像は、キャンバスの大きさに合わせて
    /// 左上寄せで貼られる。
    pub fn add_background_layer(
        &mut self,
        name: impl Into<String>,
        pixels: PixelBuffer,
    ) -> Result<usize> {
        let id = self.issue_id();
        let mut layer = if pixels.size() == self.canvas {
            Layer::background(name, pixels)
        } else {
            // 大きさが違う場合はキャンバスに合わせる（はみ出しは切る）
            let mut fitted = PixelBuffer::new(self.canvas.width, self.canvas.height);
            let rect = Rect::new(0, 0, pixels.width(), pixels.height())
                .clamp_to(fitted.bounds())
                .ok_or_else(|| PaintError::SizeMismatch {
                    given: pixels.size(),
                    expected: self.canvas,
                })?;
            fitted.blit(rect, &pixels, crate::geometry::Point::ORIGIN);
            Layer::background(name, fitted)
        };
        layer.id = id;
        self.layers.insert(0, layer);
        // 追加したぶんインデックスがずれるので、選択は「元のレイヤー」を保つ
        self.active = (self.active + 1).min(self.layers.len() - 1);
        Ok(0)
    }

    /// 下絵レイヤーをすべて取り除く。
    pub fn remove_background_layers(&mut self) {
        let active_id = self.active_layer().map(|l| l.id);
        self.layers.retain(|l| l.kind != LayerKind::Background);
        if self.layers.is_empty() {
            let id = self.issue_id();
            let mut layer = Layer::paint("レイヤー 1", self.canvas.width, self.canvas.height);
            layer.id = id;
            self.layers.push(layer);
        }
        self.active = active_id
            .and_then(|id| self.layers.iter().position(|l| l.id == id))
            .unwrap_or(self.layers.len() - 1);
    }

    /// レイヤーを削除する。
    ///
    /// 描画レイヤーが 1 枚だけのときにそれを消そうとすると
    /// [`PaintError::LastLayerNotRemovable`] を返す。
    pub fn remove_layer(&mut self, index: usize) -> Result<Layer> {
        let Some(layer) = self.layers.get(index) else {
            return Err(PaintError::LayerNotFound { index });
        };
        if layer.kind == LayerKind::Paint && self.paint_layer_count() <= 1 {
            return Err(PaintError::LastLayerNotRemovable);
        }
        let removed = self.layers.remove(index);
        self.active = self.active.min(self.layers.len().saturating_sub(1));
        Ok(removed)
    }

    /// 描画レイヤーの枚数。
    pub fn paint_layer_count(&self) -> usize {
        self.layers
            .iter()
            .filter(|l| l.kind == LayerKind::Paint)
            .count()
    }

    /// レイヤーを並べ替える（`from` を `to` の位置へ動かす）。
    pub fn move_layer(&mut self, from: usize, to: usize) -> bool {
        if from >= self.layers.len() || to >= self.layers.len() || from == to {
            return false;
        }
        let active_id = self.active_layer().map(|l| l.id);
        let layer = self.layers.remove(from);
        self.layers.insert(to, layer);
        self.active = active_id
            .and_then(|id| self.layers.iter().position(|l| l.id == id))
            .unwrap_or(self.active);
        true
    }

    /// いま描き込めるレイヤー（可変）。下絵しか無い場合はエラー。
    pub fn paint_target_mut(&mut self) -> Result<&mut Layer> {
        let active = self.active;
        match self.layers.get_mut(active) {
            Some(layer) if layer.is_editable() => Ok(layer),
            Some(layer) => Err(PaintError::LayerLocked {
                name: layer.name.clone(),
            }),
            None => Err(PaintError::LayerNotFound { index: active }),
        }
    }

    /// すべてのレイヤーを重ねた結果（PNG 書き出し・画面表示用）。
    ///
    /// 背景色は持たない（透明から始める）。表示用の市松模様などは GUI 側の責務。
    pub fn composite(&self) -> PixelBuffer {
        let mut out = PixelBuffer::new(self.canvas.width, self.canvas.height);
        self.composite_into(&mut out);
        out
    }

    /// 既存のバッファへ重ねる（大きさは切り詰められる）。
    pub fn composite_into(&self, dst: &mut PixelBuffer) {
        for layer in &self.layers {
            layer.blend_onto(dst);
        }
    }

    /// 背景色つきで合成する（書き出しプレビュー用）。
    pub fn composite_on(&self, background: Rgba) -> PixelBuffer {
        let mut out = PixelBuffer::filled(self.canvas.width, self.canvas.height, background);
        self.composite_into(&mut out);
        out
    }

    /// 下絵レイヤーを持っているか。
    pub fn has_background(&self) -> bool {
        self.layers.iter().any(|l| l.kind == LayerKind::Background)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::Point;

    fn doc(w: u32, h: u32) -> Document {
        Document::new(Size::new(w, h))
    }

    #[test]
    fn new_document_has_one_paint_layer() {
        let d = doc(8, 8);
        assert_eq!(d.layer_count(), 1);
        assert_eq!(d.paint_layer_count(), 1);
        assert!(d.active_layer().unwrap().is_editable());
        assert_eq!(d.canvas_size(), Size::new(8, 8));
        assert_eq!(d.active_index(), 0);
    }

    #[test]
    fn layer_ids_are_unique() {
        let mut d = doc(4, 4);
        d.add_paint_layer("レイヤー 2");
        let ids: Vec<_> = d.layers().iter().map(|l| l.id).collect();
        assert_eq!(ids.len(), 2);
        assert_ne!(ids[0], ids[1]);
        assert!(d.layer_by_id(ids[1]).is_some());
    }

    #[test]
    fn add_and_remove_layers() {
        let mut d = doc(4, 4);
        let idx = d.add_paint_layer("レイヤー 2");
        assert_eq!(idx, 1);
        assert_eq!(d.active_index(), 1);
        assert_eq!(d.layer_count(), 2);

        let removed = d.remove_layer(0).unwrap();
        assert!(removed.is_editable());
        assert_eq!(d.layer_count(), 1);

        // 最後の描画レイヤーは消せない
        assert!(matches!(
            d.remove_layer(0),
            Err(PaintError::LastLayerNotRemovable)
        ));
        assert!(matches!(
            d.remove_layer(9),
            Err(PaintError::LayerNotFound { .. })
        ));
    }

    #[test]
    fn background_layer_is_added_at_bottom_and_locked() {
        let mut d = doc(4, 4);
        let bg = PixelBuffer::filled(4, 4, Rgba::WHITE);
        d.add_background_layer("下絵", bg).unwrap();
        assert!(d.has_background());
        assert_eq!(d.layer_count(), 2);
        assert_eq!(d.layers()[0].kind, LayerKind::Background);
        // 選択は元の描画レイヤーのまま
        assert!(d.active_layer().unwrap().is_editable());

        assert!(matches!(
            d.layer_mut(0)
                .unwrap()
                .set_pixel(Point::ORIGIN, Rgba::BLACK),
            Err(PaintError::LayerLocked { .. })
        ));
    }

    #[test]
    fn oversized_background_is_fitted() {
        let mut d = doc(4, 4);
        let big = PixelBuffer::filled(8, 8, Rgba::WHITE);
        d.add_background_layer("下絵", big).unwrap();
        let bg = &d.layers()[0];
        assert_eq!(bg.pixels.size(), Size::new(4, 4));
        assert_eq!(bg.pixels.get(Point::new(3, 3)), Some(Rgba::WHITE));
    }

    #[test]
    fn remove_background_keeps_paint_layer() {
        let mut d = doc(4, 4);
        d.add_background_layer("下絵", PixelBuffer::filled(4, 4, Rgba::WHITE))
            .unwrap();
        d.remove_background_layers();
        assert!(!d.has_background());
        assert_eq!(d.layer_count(), 1);
        assert!(d.active_layer().unwrap().is_editable());
    }

    #[test]
    fn move_layer_changes_order_and_keeps_selection() {
        let mut d = doc(4, 4);
        d.add_paint_layer("レイヤー 2");
        let first = d.layers()[0].id;
        let second = d.layers()[1].id;
        d.set_active_index(0);
        assert!(d.move_layer(0, 1));
        assert_eq!(d.layers()[0].id, second);
        assert_eq!(d.layers()[1].id, first);
        assert_eq!(d.active_layer().unwrap().id, first);
        assert!(!d.move_layer(0, 0));
        assert!(!d.move_layer(0, 5));
    }

    #[test]
    fn composite_stacks_layers_in_order() {
        let mut d = doc(2, 2);
        d.add_background_layer("下絵", PixelBuffer::filled(2, 2, Rgba::WHITE))
            .unwrap();
        // 描画レイヤー（最前面）に 1 画素
        let idx = d.active_index();
        d.layer_mut(idx)
            .unwrap()
            .set_pixel(Point::ORIGIN, Rgba::rgb(255, 0, 0))
            .unwrap();

        let flat = d.composite();
        assert_eq!(flat.get(Point::ORIGIN), Some(Rgba::rgb(255, 0, 0)));
        assert_eq!(flat.get(Point::new(1, 1)), Some(Rgba::WHITE));

        let on_black = d.composite_on(Rgba::BLACK);
        assert_eq!(on_black.get(Point::new(1, 1)), Some(Rgba::WHITE));
    }

    #[test]
    fn composite_respects_visibility_and_opacity() {
        let mut d = doc(1, 1);
        let bg = d
            .add_background_layer("下絵", PixelBuffer::filled(1, 1, Rgba::WHITE))
            .unwrap();
        d.layer_mut(bg).unwrap().visible = false;
        assert_eq!(d.composite().get(Point::ORIGIN), Some(Rgba::TRANSPARENT));

        d.layer_mut(bg).unwrap().visible = true;
        d.layer_mut(bg).unwrap().set_opacity(0.0);
        assert_eq!(d.composite().get(Point::ORIGIN), Some(Rgba::TRANSPARENT));
    }

    #[test]
    fn paint_target_is_err_when_background_selected() {
        let mut d = doc(2, 2);
        d.add_background_layer("下絵", PixelBuffer::filled(2, 2, Rgba::WHITE))
            .unwrap();
        d.set_active_index(0);
        assert!(matches!(
            d.paint_target_mut(),
            Err(PaintError::LayerLocked { .. })
        ));
        d.set_active_index(1);
        assert!(d.paint_target_mut().is_ok());
    }
}
