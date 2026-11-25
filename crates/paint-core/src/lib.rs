//! # paint-core
//!
//! お絵描きソフトのドメイン層。
//!
//! **このクレートは他のどのクレートにも依存しない**（GUI・ファイル形式・OS に非依存）。
//! 描画エンジン（`paint-engine`）と永続化（`paint-persist`）は、このクレートが定義する
//! 型だけを使って実装される。依存関係逆転の要になる層なので、外部クレートの型を
//! 公開 API に漏らさないこと。
//!
//! ## 主な型
//!
//! - [`Rgba`] — 色（ストレートアルファ）
//! - [`Point`] / [`Size`] / [`Rect`] — 幾何
//! - [`PixelBuffer`] — RGBA8 の画素配列。境界チェック付きで、範囲外は `None` / `false`
//! - [`Layer`] / [`LayerKind`] — レイヤー（下絵は編集禁止）
//! - [`Document`] — キャンバスとレイヤー群
//! - [`ToolKind`] / [`LineWidth`] / [`ToolSettings`] — ツールの設定値

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod color;
pub mod document;
pub mod error;
pub mod geometry;
pub mod layer;
pub mod pixel_buffer;
pub mod tool;

pub use color::Rgba;
pub use document::Document;
pub use error::{PaintError, Result};
pub use geometry::{Point, Rect, Size};
pub use layer::{Layer, LayerId, LayerKind};
pub use pixel_buffer::PixelBuffer;
pub use tool::{LineWidth, Selection, ToolKind, ToolSettings};
