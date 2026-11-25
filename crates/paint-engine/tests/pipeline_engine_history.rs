//! エンジンと履歴の通しの結合テスト。
//!
//! `stamp_dot`（ツール）と `History` / `PixelPatch`（履歴）を `Document` に対して
//! 組み合わせ、「描く → 履歴へ積む → Undo/Redo」を通しで検証する。
#![allow(clippy::unwrap_used, clippy::expect_used)]

use paint_core::{Document, LineWidth, Point, Rect, Rgba, Size};
use paint_engine::{path_bounds, stamp_dot, History, PixelPatch};

/// 点を 1 つ描き、その差分を履歴へ積む（ViewModel の想定手順）。
fn draw_dot(hist: &mut History, doc: &mut Document, at: Point) {
    let width = LineWidth::W1;
    let layer_id = doc.active_layer().unwrap().id;
    let canvas = Rect::from_size(doc.canvas_size());
    let rect = path_bounds(&[at], width).unwrap().clamp_to(canvas).unwrap();
    let before = PixelPatch::snapshot(doc.active_layer().unwrap(), rect);
    stamp_dot(doc.paint_target_mut().unwrap(), at, width, Rgba::BLACK).unwrap();
    let after = PixelPatch::snapshot(doc.active_layer().unwrap(), rect);
    hist.push(
        Box::new(PixelPatch::new(layer_id, rect, before, after)),
        doc,
    );
}

/// 11. 100 ストローク → 100 回 undo で「描く前のスナップショットと完全一致」。
#[test]
fn hundred_strokes_undo_to_initial_snapshot() {
    let mut doc = Document::new(Size::new(32, 32));
    let initial = doc.composite();
    let mut hist = History::new(150);

    for i in 0..100i32 {
        draw_dot(&mut hist, &mut doc, Point::new(i % 32, i / 32));
    }
    assert_eq!(hist.stats().undoable, 100);
    assert_eq!(hist.stats().redoable, 0);
    assert_eq!(hist.limit(), 150);
    assert_ne!(doc.composite(), initial);

    for _ in 0..100 {
        assert!(hist.undo(&mut doc));
    }
    assert_eq!(doc.composite().data(), initial.data());
    assert_eq!(hist.stats().undoable, 0);
    assert!(!hist.undo(&mut doc));
}

/// 12. 一部のストロークだけ undo しても、他のストロークの結果は残る。
#[test]
fn undoing_one_stroke_keeps_the_others() {
    let mut doc = Document::new(Size::new(32, 32));
    let mut hist = History::new(10);
    let a = Point::new(2, 2);
    let b = Point::new(20, 2);
    let c = Point::new(2, 20);
    draw_dot(&mut hist, &mut doc, a);
    draw_dot(&mut hist, &mut doc, b);
    draw_dot(&mut hist, &mut doc, c);
    assert_eq!(hist.stats().undoable, 3);

    assert!(hist.undo(&mut doc)); // 最後の c を取り消す
    let flat = doc.composite();
    assert!(flat.get(a).is_some_and(|p| p.a > 0), "a は残る");
    assert!(flat.get(b).is_some_and(|p| p.a > 0), "b は残る");
    assert_eq!(flat.get(c), Some(Rgba::TRANSPARENT), "c だけ消える");
}

/// 13. 手で作った PixelPatch を push し、undo/redo で前後の画素が完全一致。
#[test]
fn hand_made_patch_undo_redo_round_trips() {
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

    let mut hist = History::new(10);
    hist.push(
        Box::new(PixelPatch::new(
            layer_id,
            rect,
            before.clone(),
            after.clone(),
        )),
        &mut doc,
    );
    assert_eq!(
        doc.active_layer().unwrap().pixels.crop(rect).unwrap(),
        after
    );

    assert!(hist.undo(&mut doc));
    assert_eq!(
        doc.active_layer().unwrap().pixels.crop(rect).unwrap(),
        before
    );
    assert!(hist.redo(&mut doc));
    assert_eq!(
        doc.active_layer().unwrap().pixels.crop(rect).unwrap(),
        after
    );
}

/// 14. 対象レイヤーを削除した後に undo してもパニックせず、他のレイヤーも壊れない。
#[test]
fn undo_after_target_layer_removed_is_safe() {
    let mut doc = Document::new(Size::new(8, 8));
    doc.add_paint_layer("レイヤー 2");
    let target_id = doc.active_layer().unwrap().id; // 追加したレイヤー（index 1）
    let rect = Rect::new(1, 1, 2, 2);
    let before = PixelPatch::snapshot(doc.active_layer().unwrap(), rect);
    doc.paint_target_mut()
        .unwrap()
        .set_pixel(Point::new(1, 1), Rgba::rgb(50, 60, 70))
        .unwrap();
    let after = PixelPatch::snapshot(doc.active_layer().unwrap(), rect);

    let mut hist = History::new(10);
    hist.push(
        Box::new(PixelPatch::new(target_id, rect, before, after)),
        &mut doc,
    );

    let keep = doc.layer(0).unwrap().clone();
    doc.remove_layer(1).unwrap();
    assert_eq!(doc.layer_count(), 1);

    // 対象が消えていてもパニックせず、他のレイヤーは変化しない。
    assert!(hist.undo(&mut doc));
    assert_eq!(doc.layer(0).unwrap(), &keep);
    assert!(hist.redo(&mut doc));
    assert_eq!(doc.layer(0).unwrap(), &keep);
}
