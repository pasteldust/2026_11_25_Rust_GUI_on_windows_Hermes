//! エラー型。

use crate::geometry::Point;

/// ドメイン層のエラー。
///
/// 「利用者にそのまま見せられる日本語のメッセージ」を持つことを方針とする
/// （GUI のステータスバーにそのまま出す）。
#[derive(Debug, thiserror::Error, PartialEq, Eq, Clone)]
pub enum PaintError {
    /// 範囲外の座標にアクセスした。
    #[error("座標 ({x}, {y}) はキャンバスの外です")]
    OutOfBounds {
        /// X 座標。
        x: i32,
        /// Y 座標。
        y: i32,
    },

    /// 編集禁止レイヤー（下絵）へ書き込もうとした。
    #[error("レイヤー「{name}」は編集できません（下絵レイヤー）")]
    LayerLocked {
        /// レイヤー名。
        name: String,
    },

    /// 指定したインデックスのレイヤーが存在しない。
    #[error("レイヤー {index} が見つかりません")]
    LayerNotFound {
        /// 指定されたインデックス。
        index: usize,
    },

    /// 画像サイズがキャンバスと一致しない。
    #[error("画像の大きさ {given:?} がキャンバス {expected:?} と一致しません")]
    SizeMismatch {
        /// 与えられた大きさ。
        given: crate::geometry::Size,
        /// 期待した大きさ。
        expected: crate::geometry::Size,
    },

    /// 大きさが 0、または上限を超えている。
    #[error("扱えない大きさです: {width}x{height}（1〜{max} の範囲で指定してください）")]
    InvalidSize {
        /// 幅。
        width: u32,
        /// 高さ。
        height: u32,
        /// 上限。
        max: u32,
    },

    /// 編集できるレイヤーが 1 枚も無い。
    #[error("編集できるレイヤーがありません。描画レイヤーを追加してください")]
    NoEditableLayer,

    /// 最後の描画レイヤーは削除できない。
    #[error("最後の描画レイヤーは削除できません")]
    LastLayerNotRemovable,

    /// 色の文字列が解釈できなかった。
    #[error("色として解釈できません: {0}")]
    InvalidColor(String),

    /// データの大きさが足りない、または余っている。
    #[error("データの大きさが不正です（期待 {expected} バイト、実際 {actual} バイト）")]
    InvalidDataLength {
        /// 期待した長さ。
        expected: usize,
        /// 実際の長さ。
        actual: usize,
    },
}

impl PaintError {
    /// 範囲外座標のエラーを作る。
    pub fn out_of_bounds(p: Point) -> Self {
        Self::OutOfBounds { x: p.x, y: p.y }
    }
}

/// ドメイン層の結果型。
pub type Result<T> = std::result::Result<T, PaintError>;
