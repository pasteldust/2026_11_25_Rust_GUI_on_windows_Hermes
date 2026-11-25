//! 起動口。
//!
//! - 引数なし: GUI を開く
//! - `--self-test <出力先>`: GUI を開かずに自己テストを実行し、結果を JSON で出す
//!   （CI の E2E と、手元での動作確認に使う）

use std::path::PathBuf;
use std::process::ExitCode;

use paint_app::app::PainterApp;
use paint_app::view_model::PainterViewModel;
use paint_core::Size;

/// 既定のキャンバスの大きさ。
const DEFAULT_CANVAS: Size = Size::new(1280, 720);

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();

    if let Some(index) = args.iter().position(|a| a == "--self-test") {
        let dir = args
            .get(index + 1)
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."));
        return match paint_app::self_test::run(&dir) {
            Ok(report) if report.ok => ExitCode::SUCCESS,
            Ok(report) => {
                eprintln!("自己テストに失敗しました: {dir:?}");
                for step in report.steps.iter().filter(|s| !s.ok) {
                    eprintln!("  - {}: {}", step.name, step.detail);
                }
                ExitCode::FAILURE
            }
            Err(e) => {
                eprintln!("自己テストを実行できません: {e}");
                ExitCode::FAILURE
            }
        };
    }

    let view_model = PainterViewModel::new(DEFAULT_CANVAS);
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size([800.0, 600.0])
            .with_title("お絵描き"),
        ..Default::default()
    };
    match eframe::run_native(
        "お絵描き",
        options,
        Box::new(move |cc| {
            // 日本語の字形は egui に同梱されていないので、OS のフォントを借りる
            if paint_app::app::install_japanese_font(&cc.egui_ctx).is_none() {
                eprintln!(
                    "日本語フォントが見つかりませんでした。日本語は豆腐（□）で表示されます。"
                );
            }
            Ok(Box::new(PainterApp::new(view_model)))
        }),
    ) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("起動できませんでした: {e}");
            ExitCode::FAILURE
        }
    }
}
