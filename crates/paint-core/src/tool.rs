//! ツールの設定値（レイヤー・ツールの種類・線幅・選択範囲）。

use crate::color::Rgba;
use crate::geometry::Rect;

/// ツールの種類。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ToolKind {
    /// 自由線（ドラッグで描く）。
    Pen,
    /// 点（クリックで 1 点）。
    Dot,
    /// 直線（始点と終点を指定）。Shift を押している間だけ有効。
    Line,
    /// 消しゴム（ドラッグで透明にする）。
    Eraser,
    /// 範囲消去（矩形を選んでから Delete）。
    RectSelect,
}

impl ToolKind {
    /// 画面に出す名前。
    pub const fn label(self) -> &'static str {
        match self {
            Self::Pen => "ペン",
            Self::Dot => "点",
            Self::Line => "直線（Shift）",
            Self::Eraser => "消しゴム",
            Self::RectSelect => "範囲消去",
        }
    }

    /// すべてのツール（UI の並び順）。
    pub const fn all() -> [Self; 5] {
        [
            Self::Pen,
            Self::Dot,
            Self::Line,
            Self::Eraser,
            Self::RectSelect,
        ]
    }
}

/// 選べる線幅。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum LineWidth {
    /// 1 px。
    W1,
    /// 2 px。
    W2,
    /// 4 px。
    W4,
    /// 8 px。
    W8,
    /// 16 px。
    W16,
    /// 32 px。
    W32,
}

impl LineWidth {
    /// 画素数。
    pub const fn pixels(self) -> u32 {
        match self {
            Self::W1 => 1,
            Self::W2 => 2,
            Self::W4 => 4,
            Self::W8 => 8,
            Self::W16 => 16,
            Self::W32 => 32,
        }
    }

    /// 一覧（UI の並び順）。
    pub const fn all() -> [Self; 6] {
        [Self::W1, Self::W2, Self::W4, Self::W8, Self::W16, Self::W32]
    }

    /// 保存形式で使うタグ。
    pub const fn tag(self) -> u8 {
        match self {
            Self::W1 => 1,
            Self::W2 => 2,
            Self::W4 => 4,
            Self::W8 => 8,
            Self::W16 => 16,
            Self::W32 => 32,
        }
    }

    /// タグから戻す（保存ファイルの読み込み用）。
    pub const fn from_tag(tag: u8) -> Option<Self> {
        match tag {
            1 => Some(Self::W1),
            2 => Some(Self::W2),
            4 => Some(Self::W4),
            8 => Some(Self::W8),
            16 => Some(Self::W16),
            32 => Some(Self::W32),
            _ => None,
        }
    }
}

impl std::fmt::Display for LineWidth {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}px", self.pixels())
    }
}

/// 現在のツール設定（ViewModel が保持する）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ToolSettings {
    /// 選んでいるツール。
    pub tool: ToolKind,
    /// 線幅。
    pub width: LineWidth,
    /// 色。
    pub color: Rgba,
}

impl Default for ToolSettings {
    fn default() -> Self {
        Self {
            tool: ToolKind::Pen,
            width: LineWidth::W1,
            color: Rgba::BLACK,
        }
    }
}

/// 矩形の選択範囲（範囲消去に使う）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Selection {
    /// 選択中の矩形。`None` なら未選択。
    pub rect: Option<Rect>,
}

impl Selection {
    /// 未選択。
    pub const fn none() -> Self {
        Self { rect: None }
    }

    /// 矩形を選ぶ（正規化して保持する）。
    pub fn new(rect: Rect) -> Self {
        if rect.is_empty() {
            Self::none()
        } else {
            Self { rect: Some(rect) }
        }
    }

    /// 選択を解除する。
    pub fn clear(&mut self) {
        self.rect = None;
    }

    /// 選択中か。
    pub const fn is_active(&self) -> bool {
        self.rect.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::Point;

    #[test]
    fn line_width_tags_round_trip() {
        for w in LineWidth::all() {
            assert_eq!(LineWidth::from_tag(w.tag()), Some(w));
            assert_eq!(w.pixels(), u32::from(w.tag()));
        }
        assert_eq!(LineWidth::from_tag(3), None);
        assert_eq!(LineWidth::W8.to_string(), "8px");
    }

    #[test]
    fn tool_list_is_complete() {
        let all = ToolKind::all();
        assert_eq!(all.len(), 5);
        assert!(all.contains(&ToolKind::Pen));
        assert!(all.contains(&ToolKind::Dot));
        assert!(all.contains(&ToolKind::Line));
        assert!(all.contains(&ToolKind::Eraser));
        assert!(all.contains(&ToolKind::RectSelect));
        assert!(ToolKind::all().iter().all(|t| !t.label().is_empty()));
    }

    #[test]
    fn selection_normalizes_empty() {
        assert!(!Selection::new(Rect::new(0, 0, 0, 5)).is_active());
        assert!(!Selection::default().is_active());
        let mut s = Selection::new(Rect::from_points(Point::new(3, 3), Point::new(1, 1)));
        assert!(s.is_active());
        assert_eq!(s.rect, Some(Rect::new(1, 1, 3, 3)));
        s.clear();
        assert!(!s.is_active());
    }

    #[test]
    fn default_settings() {
        let s = ToolSettings::default();
        assert_eq!(s.tool, ToolKind::Pen);
        assert_eq!(s.width, LineWidth::W1);
        assert_eq!(s.color, Rgba::BLACK);
    }
}
