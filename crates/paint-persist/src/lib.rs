//! # paint-persist
//!
//! 永続化。独自形式 `.paint` の保存・読込と、画像（PNG/JPEG/BMP）の読み込み・
//! PNG 書き出しを担当する。
//!
//! ファイル形式は先頭にマジックとバージョンを持ち、将来のフィールド追加に耐える。
//! 形式の詳細は [`project`] モジュールのドキュメントコメントを参照。
//!
//! - [`save_project`] / [`load_project`] — `.paint` の読み書き（原子的な保存）
//! - [`load_image`] / [`export_png`] — 画像の入出力
//! - [`PersistError`] — この層のエラー型（日本語メッセージ付き）

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod error;
pub mod image_io;
pub mod project;

pub use error::PersistError;
pub use image_io::{export_png, export_png_bytes, load_image, load_image_bytes, ExportOptions};
pub use project::{
    load_project, load_project_bytes, save_project, save_project_bytes, PROJECT_EXTENSION,
    PROJECT_MAGIC, PROJECT_VERSION,
};
