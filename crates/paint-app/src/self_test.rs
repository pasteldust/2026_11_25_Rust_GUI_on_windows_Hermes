//! GUI を開かない自己テスト（CI の E2E 用）。
//!
//! Windows の CI ランナーではウィンドウを開けないため、
//! 「起動 → 下絵読み込み → 描画 → 保存 → 再読込 → Undo/Redo → PNG 書き出し」を
//! GUI 抜きで実行し、結果を JSON で書き出す。1 つでも失敗すれば `ok = false`。

use std::path::Path;
use std::time::Instant;

use paint_core::{Document, LayerKind, LineWidth, PixelBuffer, Point, Rgba, Size, ToolKind};
use paint_persist::{
    export_png_bytes, load_image_bytes, load_project_bytes, save_project_bytes, ExportOptions,
};
use serde_json::json;

use crate::view_model::PainterViewModel;

/// 1 ステップの結果。
#[derive(Debug, Clone)]
pub struct StepReport {
    /// ステップ名。
    pub name: String,
    /// 成功したか。
    pub ok: bool,
    /// 補足（失敗理由や数値）。
    pub detail: String,
}

/// 自己テスト全体の結果。
#[derive(Debug, Clone)]
pub struct SelfTestReport {
    /// すべて成功したか。
    pub ok: bool,
    /// 各ステップ。
    pub steps: Vec<StepReport>,
    /// 起動（初期化〜最初の合成）にかかったミリ秒。
    pub startup_ms: u128,
    /// 描画ステップのミリ秒。
    pub draw_ms: u128,
    /// 保存のミリ秒。
    pub save_ms: u128,
    /// 読み込みのミリ秒。
    pub load_ms: u128,
    /// ピークメモリ（MB）。取得できない環境では `None`。
    pub peak_memory_mb: Option<f64>,
}

impl SelfTestReport {
    /// JSON へ変換する。
    pub fn to_json(&self) -> serde_json::Value {
        json!({
            "ok": self.ok,
            "startup_ms": self.startup_ms,
            "draw_ms": self.draw_ms,
            "save_ms": self.save_ms,
            "load_ms": self.load_ms,
            "peak_memory_mb": self.peak_memory_mb,
            "steps": self.steps.iter().map(|s| json!({
                "name": s.name,
                "ok": s.ok,
                "detail": s.detail,
            })).collect::<Vec<_>>(),
        })
    }
}

/// ピークメモリ（MB）。Linux は `/proc/self/status` の `VmHWM` を読む。
fn peak_memory_mb() -> Option<f64> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    for line in status.lines() {
        if let Some(rest) = line.strip_prefix("VmHWM:") {
            let kb: f64 = rest.trim().trim_end_matches(" kB").trim().parse().ok()?;
            return Some((kb / 1024.0 * 10.0).round() / 10.0);
        }
    }
    None
}

/// いま選んでいる描画レイヤーの 1 画素のアルファ。
///
/// 合成（`composite`）ではなくレイヤー自身の値を見る。下絵が下にあると合成のアルファは
/// 255 になってしまうため、消しゴムの効きはレイヤーの値で確かめる。
fn paint_layer_alpha(vm: &PainterViewModel, p: Point) -> Option<u8> {
    let index = vm.document().active_index();
    let layer = vm.document().layer(index)?;
    layer.pixels.get(p).map(|c| c.a)
}

/// テスト用の PNG をその場で作る（バイナリをリポジトリに置かないため）。
fn make_test_png() -> Result<Vec<u8>, String> {
    let mut doc = Document::new(Size::new(800, 600));
    let bg = PixelBuffer::filled(800, 600, Rgba::rgb(240, 240, 250));
    doc.add_background_layer("下絵", bg)
        .map_err(|e| e.to_string())?;
    export_png_bytes(&doc, ExportOptions::default()).map_err(|e| e.to_string())
}

/// 自己テストを実行する。
///
/// `dir` に `.paint` / PNG / `selftest.json` を書き出す。
pub fn run(dir: &Path) -> std::io::Result<SelfTestReport> {
    std::fs::create_dir_all(dir)?;
    let mut steps: Vec<StepReport> = Vec::new();
    macro_rules! step {
        ($name:expr, $ok:expr, $detail:expr) => {
            steps.push(StepReport {
                name: $name.to_string(),
                ok: $ok,
                detail: $detail,
            })
        };
    }

    // 1. 起動（新しい文書を作って最初の合成まで）
    let t0 = Instant::now();
    let mut vm = PainterViewModel::new(Size::new(800, 600));
    let _ = vm.composite();
    let startup_ms = t0.elapsed().as_millis();
    step!(
        "起動",
        startup_ms < 3000,
        format!("初期化〜初回合成 {startup_ms} ms")
    );

    // 2. 下絵の読み込み
    let under_png = dir.join("under.png");
    let under_result = make_test_png().and_then(|bytes| {
        std::fs::write(&under_png, bytes).map_err(|e| e.to_string())?;
        Ok(())
    });
    let (under_ok, under_detail) = match under_result {
        Ok(()) => match vm.load_image_file(&under_png) {
            Ok(()) => (
                true,
                format!(
                    "{} を下絵レイヤーとして追加（レイヤー数 {}）",
                    under_png.display(),
                    vm.document().layer_count()
                ),
            ),
            Err(e) => (false, e),
        },
        Err(e) => (false, format!("テスト画像を作れません: {e}")),
    };
    step!("下絵読み込み", under_ok, under_detail);

    // 3. 描画（点・自由線・直線・消しゴム）
    let t1 = Instant::now();
    vm.set_tool(ToolKind::Dot);
    vm.set_line_width(LineWidth::W8);
    vm.set_color(Rgba::rgb(220, 30, 30));
    vm.pointer_down(Point::new(400, 300), false);
    vm.pointer_up(Point::new(400, 300), false);
    let dot_painted = vm
        .composite()
        .get(Point::new(400, 300))
        .is_some_and(|c| c.a > 0);

    vm.set_tool(ToolKind::Pen);
    vm.set_line_width(LineWidth::W4);
    vm.pointer_down(Point::new(50, 50), false);
    for x in (50..350).step_by(10) {
        vm.pointer_move(Point::new(x, 50 + (x - 50) / 5), false);
    }
    vm.pointer_up(Point::new(350, 110), false);
    let pen_painted = vm
        .composite()
        .get(Point::new(150, 70))
        .is_some_and(|c| c.a > 0);

    vm.set_tool(ToolKind::Line);
    vm.pointer_down(Point::new(600, 100), false);
    vm.pointer_up(Point::new(760, 100), false);
    let line_painted = vm
        .composite()
        .get(Point::new(680, 100))
        .is_some_and(|c| c.a > 0);

    vm.set_tool(ToolKind::Eraser);
    vm.set_line_width(LineWidth::W16);
    vm.pointer_down(Point::new(400, 300), false);
    vm.pointer_up(Point::new(400, 300), false);
    let erased = paint_layer_alpha(&vm, Point::new(400, 300)).is_none_or(|a| a == 0);

    let draw_ms = t1.elapsed().as_millis();
    step!(
        "描画",
        dot_painted && pen_painted && line_painted && erased,
        format!("点={dot_painted} 自由線={pen_painted} 直線={line_painted} 消しゴム={erased} ({draw_ms} ms)")
    );

    // 4. 保存
    let t2 = Instant::now();
    let project_path = dir.join("selftest.paint");
    let saved = save_project_bytes(vm.document())
        .map_err(|e| e.to_string())
        .and_then(|bytes| std::fs::write(&project_path, bytes).map_err(|e| e.to_string()));
    let save_ms = t2.elapsed().as_millis();
    let (save_ok, save_detail) = match saved {
        Ok(()) => (
            true,
            format!("{} に保存 ({save_ms} ms)", project_path.display()),
        ),
        Err(e) => (false, e),
    };
    step!("保存", save_ok, save_detail);

    // 5. 再読込（画素が一致すること）
    let t3 = Instant::now();
    let expected = vm.document().composite();
    let reloaded = std::fs::read(&project_path)
        .map_err(|e| e.to_string())
        .and_then(|bytes| load_project_bytes(&bytes).map_err(|e| e.to_string()));
    let load_ms = t3.elapsed().as_millis();
    let (reload_ok, reload_detail) = match reloaded {
        Ok(doc) => {
            let got = doc.composite();
            let same = got.data() == expected.data() && got.size() == expected.size();
            (
                same,
                if same {
                    format!("読み直して画素一致 ({load_ms} ms)")
                } else {
                    "読み直した画素が保存前と一致しません".to_string()
                },
            )
        }
        Err(e) => (false, e),
    };
    step!("再読込", reload_ok, reload_detail);

    // 6. Undo / Redo
    let before_undo = vm.document().composite();
    let undone = vm.undo();
    let after_undo = vm.document().composite();
    let changed = after_undo.data() != before_undo.data();
    let redone = vm.redo();
    let after_redo = vm.document().composite();
    let restored = after_redo.data() == before_undo.data();
    step!(
        "Undo/Redo",
        undone && changed && redone && restored,
        format!("undo={undone} 変化={changed} redo={redone} 復元={restored}")
    );

    // 7. PNG 書き出し（消した画素のアルファが保たれること）
    // 透過を確かめるため、下絵レイヤーを隠して描画レイヤーだけを書き出す
    let background_index = vm
        .document()
        .layers()
        .iter()
        .position(|layer| layer.kind == LayerKind::Background);
    if let Some(index) = background_index {
        vm.toggle_layer_visible(index);
    }
    let png_path = dir.join("selftest.png");
    let (png_ok, png_detail) = match export_png_bytes(vm.document(), ExportOptions::default()) {
        Ok(bytes) => match std::fs::write(&png_path, bytes) {
            Ok(()) => match std::fs::read(&png_path)
                .map_err(|e| e.to_string())
                .and_then(|b| load_image_bytes(&b).map_err(|e| e.to_string()))
            {
                Ok(pixels) => {
                    let alpha_zero = pixels.get(Point::new(400, 300)).is_some_and(|c| c.a == 0);
                    (
                        alpha_zero,
                        format!(
                            "{} へ書き出し、消した画素のアルファ=0: {alpha_zero}",
                            png_path.display()
                        ),
                    )
                }
                Err(e) => (false, e),
            },
            Err(e) => (false, e.to_string()),
        },
        Err(e) => (false, e.to_string()),
    };
    step!("PNG 書き出し", png_ok, png_detail);

    let ok = steps.iter().all(|s| s.ok);
    let report = SelfTestReport {
        ok,
        steps,
        startup_ms,
        draw_ms,
        save_ms,
        load_ms,
        peak_memory_mb: peak_memory_mb(),
    };

    let json = serde_json::to_string_pretty(&report.to_json()).unwrap_or_else(|_| "{}".to_string());
    std::fs::write(dir.join("selftest.json"), &json)?;
    println!("{json}");

    Ok(report)
}
