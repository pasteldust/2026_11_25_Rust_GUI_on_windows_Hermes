//! 通しの結合テスト（ViewModel 経由）。
//!
//! GUI を除く全コンポーネント（paint-core / paint-engine / paint-persist と
//! paint-app の ViewModel）を組み合わせ、「利用者操作の並び」を通しで検証する。
//! 下絵や一時ファイルは実行時に生成し、テストごとの一意なディレクトリへ置いて
//! 終了時に消す（リポジトリにバイナリを残さない）。
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::{Path, PathBuf};

use paint_app::PainterViewModel;
use paint_core::{
    Document, LayerKind, LineWidth, PaintError, PixelBuffer, Point, Rgba, Size, ToolKind,
};
use paint_persist::{export_png, load_image, ExportOptions};

/// テストごとの一時ディレクトリ（終了時に中身ごと消える）。
#[derive(Debug)]
struct TempDir {
    path: PathBuf,
}

impl TempDir {
    /// テスト名から一意なディレクトリを作る。
    fn new(test: &str) -> Self {
        let path =
            std::env::temp_dir().join(format!("paint-app-pipeline-{test}-{}", std::process::id()));
        std::fs::create_dir_all(&path).unwrap();
        Self { path }
    }

    /// ディレクトリ内のパスを作る。
    fn join(&self, name: &str) -> PathBuf {
        self.path.join(name)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// 合成結果の `p` が描かれている（不透明）か。
fn painted(vm: &mut PainterViewModel, p: Point) -> bool {
    vm.composite().get(p).is_some_and(|c| c.a > 0)
}

/// 単色の PNG を実行時に作る（下絵の読み込み用）。
fn write_solid_png(path: &Path, size: u32, color: Rgba) {
    let mut doc = Document::new(Size::new(size, size));
    doc.add_background_layer("下絵", PixelBuffer::filled(size, size, color))
        .unwrap();
    export_png(&doc, path, ExportOptions::default()).unwrap();
}

/// 1. 点を打って保存し、別の ViewModel で開くと画素が一致する。
#[test]
fn dot_saved_and_reopened_keeps_pixel() {
    let dir = TempDir::new("dot_saved_and_reopened_keeps_pixel");
    let path = dir.join("dot.paint");
    let at = Point::new(5, 6);
    let color = Rgba::rgb(12, 34, 56);

    let mut vm = PainterViewModel::new(Size::new(16, 16));
    vm.set_tool(ToolKind::Dot);
    vm.set_line_width(LineWidth::W1);
    vm.set_color(color);
    vm.pointer_down(at, false);
    vm.pointer_up(at, false);
    assert_eq!(vm.composite().get(at), Some(color));
    vm.save_project_file(&path).unwrap();

    let mut reopened = PainterViewModel::new(Size::new(4, 4));
    reopened.open_project_file(&path).unwrap();
    assert_eq!(reopened.document().canvas_size(), Size::new(16, 16));
    assert_eq!(reopened.composite().get(at), Some(color));
}

/// 2. 下絵を読み込んだ状態で描いて保存・再読込しても、下絵は編集不可のまま。
#[test]
fn underlay_stays_locked_after_round_trip() {
    let dir = TempDir::new("underlay_stays_locked_after_round_trip");
    let png = dir.join("under.png");
    let project = dir.join("under.paint");
    write_solid_png(&png, 8, Rgba::rgb(10, 20, 30));

    let mut vm = PainterViewModel::new(Size::new(8, 8));
    vm.load_image_file(&png).unwrap();
    assert!(vm.document().has_background());
    // 下絵を足しても描画レイヤーが選ばれたままなので描ける。
    vm.set_tool(ToolKind::Dot);
    vm.set_color(Rgba::rgb(200, 0, 0));
    vm.pointer_down(Point::new(1, 1), false);
    vm.pointer_up(Point::new(1, 1), false);
    vm.save_project_file(&project).unwrap();

    let mut reopened = PainterViewModel::new(Size::new(4, 4));
    reopened.open_project_file(&project).unwrap();

    let mut doc = reopened.document().clone();
    assert_eq!(doc.layer(0).unwrap().kind, LayerKind::Background);
    assert!(!doc.layer(0).unwrap().is_editable());
    assert!(matches!(
        doc.layer_mut(0)
            .unwrap()
            .set_pixel(Point::new(2, 2), Rgba::BLACK),
        Err(PaintError::LayerLocked { .. })
    ));
    assert_eq!(
        doc.layer(0).unwrap().pixels.get(Point::new(0, 0)),
        Some(Rgba::rgb(10, 20, 30)),
        "下絵の画素は無事"
    );
    assert_eq!(
        reopened.composite().get(Point::new(1, 1)),
        Some(Rgba::rgb(200, 0, 0)),
        "上に描いた点も残る"
    );
}

/// 3. 2 枚のレイヤーへ別々の色で描き、保存・再読込しても合成が同じ（上が見える）。
#[test]
fn two_layers_composite_matches_after_reopen() {
    let dir = TempDir::new("two_layers_composite_matches_after_reopen");
    let path = dir.join("two.paint");
    let at = Point::new(4, 4);
    let bottom = Rgba::rgb(0, 0, 255);
    let top = Rgba::rgb(255, 0, 0);

    let mut vm = PainterViewModel::new(Size::new(8, 8));
    vm.add_layer();
    vm.set_active_layer(0);
    vm.set_tool(ToolKind::Dot);
    vm.set_line_width(LineWidth::W4);
    vm.set_color(bottom);
    vm.pointer_down(at, false);
    vm.pointer_up(at, false);

    vm.set_active_layer(1);
    vm.set_line_width(LineWidth::W4);
    vm.set_color(top);
    vm.pointer_down(at, false);
    vm.pointer_up(at, false);

    assert_eq!(vm.composite().get(at), Some(top));
    vm.save_project_file(&path).unwrap();

    let mut reopened = PainterViewModel::new(Size::new(2, 2));
    reopened.open_project_file(&path).unwrap();
    assert_eq!(reopened.document().layer_count(), 2);
    assert_eq!(
        reopened.composite().get(at),
        Some(top),
        "上に描いた色が見える"
    );
}

/// 4. レイヤーの不透明度と表示切替が保存・再現される。
#[test]
fn layer_opacity_and_visibility_survive_round_trip() {
    let dir = TempDir::new("layer_opacity_and_visibility_survive_round_trip");
    let path = dir.join("opacity.paint");
    let at = Point::new(3, 3);

    let mut vm = PainterViewModel::new(Size::new(8, 8));
    vm.add_layer();
    vm.set_active_layer(0);
    vm.set_tool(ToolKind::Dot);
    vm.set_line_width(LineWidth::W4);
    vm.set_color(Rgba::rgb(0, 0, 255));
    vm.pointer_down(at, false);
    vm.pointer_up(at, false);

    vm.set_active_layer(1);
    vm.set_line_width(LineWidth::W4);
    vm.set_color(Rgba::rgb(255, 0, 0));
    vm.pointer_down(at, false);
    vm.pointer_up(at, false);
    vm.set_layer_opacity(1, 0.5);
    vm.toggle_layer_visible(0); // 下のレイヤーを隠す

    let before = vm.composite().get(at).unwrap();
    assert!(!vm.document().layer(0).unwrap().visible);
    assert!(
        (i32::from(before.a) - 128).abs() <= 2,
        "半透明の上レイヤーだけが見える: {before:?}"
    );

    vm.save_project_file(&path).unwrap();

    let mut reopened = PainterViewModel::new(Size::new(2, 2));
    reopened.open_project_file(&path).unwrap();
    assert!(
        !reopened.document().layer(0).unwrap().visible,
        "非表示が保存される"
    );
    assert!(reopened.document().layer(1).unwrap().visible);
    let opacity = reopened.document().layer(1).unwrap().opacity;
    assert!(
        (opacity - 0.5).abs() < 0.01,
        "不透明度 0.5 が保存される: {opacity}"
    );
    assert_eq!(reopened.composite().get(at).unwrap(), before);
}

/// 5. 60 ストローク描いて 60 回 undo すると初期状態（全画素 0）に戻る。
#[test]
fn sixty_strokes_undo_back_to_blank() {
    let mut vm = PainterViewModel::new(Size::new(32, 32));
    vm.set_tool(ToolKind::Dot);
    vm.set_line_width(LineWidth::W1);
    vm.set_color(Rgba::BLACK);
    for i in 0..60i32 {
        let p = Point::new(i % 32, i / 32);
        vm.pointer_down(p, false);
        vm.pointer_up(p, false);
    }
    assert_eq!(vm.history_stats().undoable, 60);

    for _ in 0..60 {
        assert!(vm.undo());
    }
    assert_eq!(vm.history_stats().undoable, 0);
    assert_eq!(vm.history_stats().redoable, 60);
    assert_eq!(vm.composite().data().iter().filter(|b| **b != 0).count(), 0);
}

/// 6. undo → redo で画素が完全に戻る（データ配列の完全一致）。
#[test]
fn undo_redo_restores_pixels_exactly() {
    let mut vm = PainterViewModel::new(Size::new(16, 16));
    vm.set_tool(ToolKind::Pen);
    vm.set_line_width(LineWidth::W2);
    vm.set_color(Rgba::rgb(0, 200, 0));
    vm.pointer_down(Point::new(2, 2), false);
    for x in 3..12 {
        vm.pointer_move(Point::new(x, x), false);
    }
    vm.pointer_up(Point::new(12, 12), false);

    let painted_data = vm.composite().data().to_vec();
    assert!(painted_data.iter().any(|b| *b != 0));

    assert!(vm.undo());
    let undone = vm.composite().data().to_vec();
    assert_ne!(painted_data, undone);

    assert!(vm.redo());
    assert_eq!(vm.composite().data(), painted_data.as_slice());
}

/// 7. 矩形選択 → Delete で範囲だけ消え、範囲の外は残る。
#[test]
fn rect_select_delete_clears_only_inside() {
    let mut vm = PainterViewModel::new(Size::new(16, 16));
    vm.set_tool(ToolKind::Dot);
    vm.set_line_width(LineWidth::W16);
    vm.set_color(Rgba::rgb(255, 128, 0));
    vm.pointer_down(Point::new(8, 8), false);
    vm.pointer_up(Point::new(8, 8), false);
    assert!(painted(&mut vm, Point::new(8, 8)));
    assert!(painted(&mut vm, Point::new(8, 3)));

    vm.set_tool(ToolKind::RectSelect);
    vm.pointer_down(Point::new(6, 6), false);
    vm.pointer_up(Point::new(10, 10), false);
    assert!(vm.selection().is_active());
    vm.delete_selection();

    assert!(!painted(&mut vm, Point::new(8, 8)), "選択範囲の中は消える");
    assert!(painted(&mut vm, Point::new(8, 3)), "選択範囲の外は残る");
}

/// 8. 線幅 1 と 32 で塗られる画素数を比較し、32 のほうが明確に多い。
#[test]
fn wider_line_paints_more_pixels() {
    let thin = dot_pixel_count(LineWidth::W1);
    let thick = dot_pixel_count(LineWidth::W32);
    assert!(
        thick > thin * 10,
        "32px ({thick}) は 1px ({thin}) より明確に多い"
    );
}

/// 指定の線幅で中心に 1 点描き、塗られた画素数を返す。
fn dot_pixel_count(width: LineWidth) -> usize {
    let mut vm = PainterViewModel::new(Size::new(64, 64));
    vm.set_tool(ToolKind::Dot);
    vm.set_line_width(width);
    vm.set_color(Rgba::BLACK);
    vm.pointer_down(Point::new(32, 32), false);
    vm.pointer_up(Point::new(32, 32), false);
    vm.composite()
        .data()
        .chunks_exact(4)
        .filter(|p| p[3] != 0)
        .count()
}

/// 9. 消しゴムで消した画素のアルファが 0 になり、PNG 経由でも 0 のまま。
#[test]
fn eraser_alpha_zero_survives_png_round_trip() {
    let dir = TempDir::new("eraser_alpha_zero_survives_png_round_trip");
    let png = dir.join("erased.png");
    let at = Point::new(8, 8);

    let mut vm = PainterViewModel::new(Size::new(16, 16));
    vm.set_tool(ToolKind::Dot);
    vm.set_line_width(LineWidth::W8);
    vm.set_color(Rgba::rgb(50, 100, 150));
    vm.pointer_down(at, false);
    vm.pointer_up(at, false);
    assert!(painted(&mut vm, at));

    vm.set_tool(ToolKind::Eraser);
    vm.pointer_down(at, false);
    vm.pointer_up(at, false);
    assert_eq!(vm.composite().get(at).map(|c| c.a), Some(0));

    vm.export_png_file(&png, ExportOptions::default()).unwrap();
    let reloaded = load_image(&png).unwrap();
    assert_eq!(
        reloaded.get(at).map(|c| c.a),
        Some(0),
        "PNG 経由でもアルファ 0"
    );
}

/// 10. PNG 書き出し（keep_transparency=false）で透明部分が白くなる。
#[test]
fn export_without_transparency_fills_white() {
    let dir = TempDir::new("export_without_transparency_fills_white");
    let png = dir.join("opaque.png");
    let drawn = Point::new(1, 1);
    let blank = Point::new(5, 5);

    let mut vm = PainterViewModel::new(Size::new(8, 8));
    vm.set_tool(ToolKind::Dot);
    vm.set_line_width(LineWidth::W1);
    vm.set_color(Rgba::BLACK);
    vm.pointer_down(drawn, false);
    vm.pointer_up(drawn, false);

    let options = ExportOptions {
        keep_transparency: false,
    };
    vm.export_png_file(&png, options).unwrap();
    let reloaded = load_image(&png).unwrap();
    assert_eq!(reloaded.get(blank), Some(Rgba::WHITE), "透明部分は白");
    assert_eq!(
        reloaded.get(drawn),
        Some(Rgba::BLACK),
        "描いた画素はそのまま"
    );
}
