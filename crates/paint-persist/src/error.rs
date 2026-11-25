//! 永続化層のエラー型。
//!
//! 利用者にそのまま見せられる日本語のメッセージを持つことを方針とする
//! （GUI のステータスバーにそのまま出す）。

use std::path::PathBuf;

/// 永続化層のエラー。
#[derive(Debug, thiserror::Error)]
pub enum PersistError {
    /// ファイルの読み書きに失敗した（パス付き）。
    #[error("ファイル {path} を扱えませんでした: {source}")]
    Io {
        /// 対象のパス。
        path: PathBuf,
        /// 元の入出力エラー。
        #[source]
        source: std::io::Error,
    },

    /// 画像のデコード・エンコードに失敗した。
    #[error("画像を読み書きできませんでした: {source}")]
    Image {
        /// 対象のパス（メモリ上のデータの場合は `None`）。
        path: Option<PathBuf>,
        /// 元の画像エラー。
        #[source]
        source: image::ImageError,
    },

    /// 先頭のマジックが一致しない（`.paint` ではない）。
    #[error("この形式のファイルではありません（.paint ではありません）")]
    BadMagic,

    /// 対応していない形式バージョン。
    #[error("未対応の形式バージョンです（{found}、対応は {supported}）")]
    UnsupportedVersion {
        /// ファイルに書かれていたバージョン。
        found: u16,
        /// この実装が対応しているバージョン。
        supported: u16,
    },

    /// データの内容が壊れている。
    #[error("ファイルが壊れています: {detail}")]
    Corrupt {
        /// 何が壊れているかの詳細。
        detail: String,
    },

    /// ドメイン層（paint-core）から返されたエラー。
    #[error("{0}")]
    Project(#[from] paint_core::PaintError),

    /// 画像が扱える上限を超えている。
    #[error("画像が大きすぎます: {width}x{height}（上限は 1 辺 {max} 画素）")]
    TooLarge {
        /// 幅。
        width: u32,
        /// 高さ。
        height: u32,
        /// 1 辺の上限。
        max: u32,
    },
}

/// 永続化層の結果型。
pub type Result<T> = std::result::Result<T, PersistError>;
