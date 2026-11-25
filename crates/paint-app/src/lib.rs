//! # paint-app
//!
//! GUI アプリケーション。ViewModel（UI 非依存）と egui/eframe による画面。
//!
//! ViewModel は描画要求をイベント列として返し、GUI はそれを描くだけにする。
//! `--self-test` を付けると GUI を開かずに一連の操作を実行する（CI 用）。

#![forbid(unsafe_code)]
#![warn(missing_docs)]
// テストでは unwrap / expect を平気で使う（失敗したらその場で分かるほうがよい）
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

pub mod app;
pub mod self_test;
pub mod view_model;

pub use view_model::PainterViewModel;
