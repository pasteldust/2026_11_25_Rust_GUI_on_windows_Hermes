//! 履歴（Undo/Redo）とコマンド。
//!
//! 描画は「変更のあった矩形だけ」を前後で保存する差分コマンド
//! [`PixelPatch`] として積む。全面スナップショットは使わない。

use paint_core::{Document, Layer, LayerId, PixelBuffer, Point, Rect};

/// 取り消し可能な操作。
pub trait Command: std::fmt::Debug + Send {
    /// 実行する。
    fn apply(&mut self, doc: &mut Document);
    /// 取り消す。
    fn revert(&mut self, doc: &mut Document);
    /// 履歴に表示する名前。
    fn name(&self) -> &str;
}

/// 変更矩形の前後の画素を持つ差分コマンド。
///
/// 対象レイヤーを ID で覚えておくので、途中でレイヤーを並べ替えても
/// 正しいレイヤーへ書き戻せる。対象レイヤーが消えていた場合は何もしない。
#[derive(Debug)]
pub struct PixelPatch {
    layer_id: LayerId,
    rect: Rect,
    before: PixelBuffer,
    after: PixelBuffer,
    name: String,
}

impl PixelPatch {
    /// `layer` の `rect` 部分を切り出す（はみ出しは切り詰める）。
    ///
    /// 完全に外側で切り出せない場合は 1x1 の透明バッファを返す（パニックしない）。
    pub fn snapshot(layer: &Layer, rect: Rect) -> PixelBuffer {
        match rect
            .clamp_to(layer.pixels.bounds())
            .and_then(|r| layer.pixels.crop(r))
        {
            Some(buf) => buf,
            None => PixelBuffer::new(1, 1),
        }
    }

    /// 変更矩形・前後の画素から作る。
    pub fn new(layer_id: LayerId, rect: Rect, before: PixelBuffer, after: PixelBuffer) -> Self {
        Self {
            layer_id,
            rect,
            before,
            after,
            name: "描画".to_string(),
        }
    }

    /// 変更矩形。
    pub fn rect(&self) -> Rect {
        self.rect
    }

    /// 対象レイヤーの ID。
    pub fn layer_id(&self) -> LayerId {
        self.layer_id
    }

    /// `src` を対象レイヤーの `rect` へ書き戻す（レイヤーが無ければ何もしない）。
    fn blit_into(doc: &mut Document, layer_id: LayerId, rect: Rect, src: &PixelBuffer) {
        let index = doc.layers().iter().position(|l| l.id == layer_id);
        if let Some(i) = index {
            if let Some(layer) = doc.layer_mut(i) {
                layer.pixels.blit(rect, src, Point::ORIGIN);
            }
        }
    }
}

impl Command for PixelPatch {
    fn apply(&mut self, doc: &mut Document) {
        Self::blit_into(doc, self.layer_id, self.rect, &self.after);
    }

    fn revert(&mut self, doc: &mut Document) {
        Self::blit_into(doc, self.layer_id, self.rect, &self.before);
    }

    fn name(&self) -> &str {
        &self.name
    }
}

/// 履歴の統計（テスト・UI 表示用）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct HistoryStats {
    /// Undo できる回数。
    pub undoable: usize,
    /// Redo できる回数。
    pub redoable: usize,
}

/// [`History`] の既定上限。
pub const DEFAULT_LIMIT: usize = 100;

/// Undo/Redo の履歴。
#[derive(Debug)]
pub struct History {
    undo_stack: Vec<Box<dyn Command>>,
    redo_stack: Vec<Box<dyn Command>>,
    limit: usize,
}

impl History {
    /// 上限つきで作る（`limit` は最低 1 に丸められる）。
    pub fn new(limit: usize) -> Self {
        Self {
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            limit: limit.max(1),
        }
    }

    /// コマンドを実行して履歴に積む。新しい操作で Redo は破棄され、
    /// 上限を超えた古いコマンドは捨てられる。
    pub fn push(&mut self, mut cmd: Box<dyn Command>, doc: &mut Document) {
        cmd.apply(doc);
        self.undo_stack.push(cmd);
        self.redo_stack.clear();
        while self.undo_stack.len() > self.limit {
            self.undo_stack.remove(0);
        }
    }

    /// 元に戻す。戻せたら `true`。
    pub fn undo(&mut self, doc: &mut Document) -> bool {
        match self.undo_stack.pop() {
            Some(mut cmd) => {
                cmd.revert(doc);
                self.redo_stack.push(cmd);
                true
            }
            None => false,
        }
    }

    /// やり直す。やり直せたら `true`。
    pub fn redo(&mut self, doc: &mut Document) -> bool {
        match self.redo_stack.pop() {
            Some(mut cmd) => {
                cmd.apply(doc);
                self.undo_stack.push(cmd);
                true
            }
            None => false,
        }
    }

    /// 履歴を空にする。
    pub fn clear(&mut self) {
        self.undo_stack.clear();
        self.redo_stack.clear();
    }

    /// 統計。
    pub fn stats(&self) -> HistoryStats {
        HistoryStats {
            undoable: self.undo_stack.len(),
            redoable: self.redo_stack.len(),
        }
    }

    /// 上限。
    pub fn limit(&self) -> usize {
        self.limit
    }
}

impl Default for History {
    fn default() -> Self {
        Self::new(DEFAULT_LIMIT)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::{path_bounds, stamp_dot};
    use paint_core::{LineWidth, Rgba, Size};

    /// 画素が 1 つでも透明でないなら `true`。
    fn has_paint(buf: &PixelBuffer) -> bool {
        buf.data()
            .chunks_exact(4)
            .any(|p| p[3] != 0 || p[0] != 0 || p[1] != 0 || p[2] != 0)
    }

    fn canvas_rect(doc: &Document) -> Rect {
        let s = doc.canvas_size();
        Rect::new(0, 0, s.width, s.height)
    }

    /// 点を 1 つ描き、その差分を履歴へ積む（ViewModel の想定手順）。
    fn draw_dot(hist: &mut History, doc: &mut Document, at: Point, color: Rgba) {
        let width = LineWidth::W1;
        let layer_id = doc.active_layer().unwrap().id;
        let rect = path_bounds(&[at], width)
            .unwrap()
            .clamp_to(canvas_rect(doc))
            .unwrap();
        let before = PixelPatch::snapshot(doc.active_layer().unwrap(), rect);
        stamp_dot(doc.paint_target_mut().unwrap(), at, width, color).unwrap();
        let after = PixelPatch::snapshot(doc.active_layer().unwrap(), rect);
        hist.push(
            Box::new(PixelPatch::new(layer_id, rect, before, after)),
            doc,
        );
    }

    #[test]
    fn patch_apply_and_revert_round_trip() {
        let mut doc = Document::new(Size::new(8, 8));
        let layer_id = doc.active_layer().unwrap().id;
        let rect = Rect::new(2, 2, 3, 3);
        let before = PixelPatch::snapshot(doc.active_layer().unwrap(), rect);

        for y in 2..5 {
            for x in 2..5 {
                doc.paint_target_mut()
                    .unwrap()
                    .set_pixel(Point::new(x, y), Rgba::rgb(10, 20, 30))
                    .unwrap();
            }
        }
        let after = PixelPatch::snapshot(doc.active_layer().unwrap(), rect);
        assert_ne!(before, after);

        let mut patch = PixelPatch::new(layer_id, rect, before.clone(), after.clone());
        assert_eq!(patch.rect(), rect);
        assert_eq!(patch.layer_id(), layer_id);
        assert_eq!(patch.name(), "描画");

        patch.revert(&mut doc);
        assert_eq!(
            doc.active_layer().unwrap().pixels.crop(rect).unwrap(),
            before
        );
        patch.apply(&mut doc);
        assert_eq!(
            doc.active_layer().unwrap().pixels.crop(rect).unwrap(),
            after
        );
    }

    #[test]
    fn patch_snapshot_outside_is_safe() {
        let doc = Document::new(Size::new(4, 4));
        let layer = doc.active_layer().unwrap();
        // 完全に外側でもパニックせず 1x1 を返す
        let buf = PixelPatch::snapshot(layer, Rect::new(100, 100, 5, 5));
        assert_eq!(buf.size(), Size::new(1, 1));
        // はみ出しは切り詰められる
        let clipped = PixelPatch::snapshot(layer, Rect::new(-2, -2, 4, 4));
        assert_eq!(clipped.size(), Size::new(2, 2));
    }

    #[test]
    fn patch_on_missing_layer_is_noop() {
        let mut doc = Document::new(Size::new(4, 4));
        let mut patch = PixelPatch::new(
            LayerId(9999),
            Rect::new(0, 0, 1, 1),
            PixelBuffer::new(1, 1),
            PixelBuffer::filled(1, 1, Rgba::WHITE),
        );
        let snapshot = doc.composite();
        patch.apply(&mut doc);
        patch.revert(&mut doc);
        assert_eq!(doc.composite(), snapshot);
    }

    #[test]
    fn history_undoes_sixty_draws() {
        let mut doc = Document::new(Size::new(32, 32));
        let mut hist = History::new(DEFAULT_LIMIT);
        let initial = doc.composite();

        for i in 0..60 {
            let at = Point::new(i % 32, i / 32);
            draw_dot(&mut hist, &mut doc, at, Rgba::BLACK);
        }
        assert_ne!(doc.composite(), initial);
        assert_eq!(hist.stats().undoable, 60);

        for _ in 0..60 {
            assert!(hist.undo(&mut doc));
        }
        assert_eq!(doc.composite(), initial);
        assert!(!hist.undo(&mut doc));
    }

    #[test]
    fn history_redo_and_branching() {
        let mut doc = Document::new(Size::new(16, 16));
        let mut hist = History::default();
        draw_dot(&mut hist, &mut doc, Point::new(3, 3), Rgba::BLACK);
        let after_first = doc.composite();

        assert!(hist.undo(&mut doc));
        assert_eq!(hist.stats().redoable, 1);
        assert!(hist.redo(&mut doc));
        assert_eq!(doc.composite(), after_first);
        assert_eq!(hist.stats().redoable, 0);

        // undo のあとに新しい操作をすると redo できなくなる
        assert!(hist.undo(&mut doc));
        assert_eq!(hist.stats().redoable, 1);
        draw_dot(&mut hist, &mut doc, Point::new(9, 9), Rgba::BLACK);
        assert_eq!(hist.stats().redoable, 0);
        assert!(!hist.redo(&mut doc));
    }

    #[test]
    fn history_limit_discards_oldest() {
        let mut doc = Document::new(Size::new(64, 64));
        let mut hist = History::new(10);
        for i in 0..25 {
            draw_dot(&mut hist, &mut doc, Point::new(i, 0), Rgba::BLACK);
        }
        assert_eq!(hist.stats().undoable, 10);
        let mut undone = 0;
        while hist.undo(&mut doc) {
            undone += 1;
        }
        assert_eq!(undone, 10);
        assert!(hist.stats().undoable <= 10);
    }

    #[test]
    fn history_clear_and_stats() {
        let mut doc = Document::new(Size::new(8, 8));
        let mut hist = History::new(5);
        assert_eq!(hist.limit(), 5);
        assert_eq!(hist.stats(), HistoryStats::default());
        assert!(!hist.undo(&mut doc));
        assert!(!hist.redo(&mut doc));

        draw_dot(&mut hist, &mut doc, Point::new(1, 1), Rgba::BLACK);
        assert!(has_paint(&doc.composite()));
        hist.clear();
        assert_eq!(hist.stats(), HistoryStats::default());
        // 履歴を消しても画素は残る
        assert!(has_paint(&doc.composite()));
    }

    #[test]
    fn default_history_has_default_limit() {
        let hist = History::default();
        assert_eq!(hist.limit(), DEFAULT_LIMIT);
    }
}
