//! # paint-engine
//!
//! 描画エンジン。ツール（ペン・点・直線・消しゴム・範囲消去）と Undo/Redo を担当する。
//!
//! GUI にもファイル形式にも依存しない。ポインタ入力は座標と修飾キーの真偽値として受け取り、
//! 「どう描くか」だけを決める。描画は変更のあった矩形だけを抜き出した差分コマンド
//! [`history::PixelPatch`] として履歴に積む（全面スナップショットは使わない）。

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod history;
pub mod tools;

pub use history::{Command, History, HistoryStats, PixelPatch};
pub use tools::{
    draw_path, erase_line, erase_rect, line_bounds, path_bounds, stamp_dot, stroke_line,
    StrokeBuilder,
};
