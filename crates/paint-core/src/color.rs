//! 色（ストレートアルファの RGBA8）。

use crate::error::PaintError;

/// RGBA8 の色。アルファは**ストレート**（乗算済みではない）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Rgba {
    /// 赤。
    pub r: u8,
    /// 緑。
    pub g: u8,
    /// 青。
    pub b: u8,
    /// 不透明度（0 = 完全な透明）。
    pub a: u8,
}

impl Rgba {
    /// 完全な透明。
    pub const TRANSPARENT: Self = Self {
        r: 0,
        g: 0,
        b: 0,
        a: 0,
    };
    /// 黒（不透明）。
    pub const BLACK: Self = Self {
        r: 0,
        g: 0,
        b: 0,
        a: 255,
    };
    /// 白（不透明）。
    pub const WHITE: Self = Self {
        r: 255,
        g: 255,
        b: 255,
        a: 255,
    };

    /// 4 成分から作る。
    pub const fn new(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    /// 不透明色を作る。
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }

    /// 配列へ。
    pub const fn to_array(self) -> [u8; 4] {
        [self.r, self.g, self.b, self.a]
    }

    /// 配列から。
    pub const fn from_array(v: [u8; 4]) -> Self {
        Self {
            r: v[0],
            g: v[1],
            b: v[2],
            a: v[3],
        }
    }

    /// アルファだけ差し替える。
    pub const fn with_alpha(self, a: u8) -> Self {
        Self { a, ..self }
    }

    /// 完全に透明か。
    pub const fn is_transparent(self) -> bool {
        self.a == 0
    }

    /// `#RRGGBB` / `#RRGGBBAA` / `RRGGBB` / `RRGGBBAA` を読む。
    pub fn from_hex(s: &str) -> Result<Self, PaintError> {
        let t = s.trim().trim_start_matches('#');
        let bad = || PaintError::InvalidColor(s.to_string());
        let parse = |part: &str| u8::from_str_radix(part, 16).map_err(|_| bad());
        match t.len() {
            6 => Ok(Self::rgb(
                parse(&t[0..2])?,
                parse(&t[2..4])?,
                parse(&t[4..6])?,
            )),
            8 => Ok(Self::new(
                parse(&t[0..2])?,
                parse(&t[2..4])?,
                parse(&t[4..6])?,
                parse(&t[6..8])?,
            )),
            _ => Err(bad()),
        }
    }

    /// `#RRGGBB`（アルファは落とす）。
    pub fn to_hex(self) -> String {
        format!("#{:02X}{:02X}{:02X}", self.r, self.g, self.b)
    }

    /// この色を `dst` の上に重ねる（source-over、ストレートアルファ）。
    ///
    /// 不透明なら `self`、透明なら `dst` をそのまま返す。
    pub fn blend_over(self, dst: Self) -> Self {
        if self.a == 255 {
            return self;
        }
        if self.a == 0 {
            return dst;
        }
        let sa = u32::from(self.a);
        let da = u32::from(dst.a);
        let out_a = sa + da * (255 - sa) / 255;
        if out_a == 0 {
            return Self::TRANSPARENT;
        }
        let mix = |s: u8, d: u8| -> u8 {
            let num = u32::from(s) * sa * 255 + u32::from(d) * da * (255 - sa);
            (num / (out_a * 255)) as u8
        };
        Self {
            r: mix(self.r, dst.r),
            g: mix(self.g, dst.g),
            b: mix(self.b, dst.b),
            a: out_a as u8,
        }
    }

    /// `factor`（0.0〜1.0）でこの色の不透明度を弱める。
    pub fn scaled_alpha(self, factor: f32) -> Self {
        let f = factor.clamp(0.0, 1.0);
        self.with_alpha((f32::from(self.a) * f).round() as u8)
    }
}

/// カラーパレット（GUI の色選択に使う既定の並び）。
pub const PALETTE: [Rgba; 16] = [
    Rgba::new(0, 0, 0, 255),
    Rgba::new(255, 255, 255, 255),
    Rgba::new(128, 128, 128, 255),
    Rgba::new(255, 0, 0, 255),
    Rgba::new(255, 128, 0, 255),
    Rgba::new(255, 255, 0, 255),
    Rgba::new(0, 192, 0, 255),
    Rgba::new(0, 128, 255, 255),
    Rgba::new(0, 0, 255, 255),
    Rgba::new(128, 0, 255, 255),
    Rgba::new(255, 0, 255, 255),
    Rgba::new(255, 128, 128, 255),
    Rgba::new(128, 64, 0, 255),
    Rgba::new(0, 255, 255, 255),
    Rgba::new(64, 64, 64, 255),
    Rgba::new(200, 200, 200, 255),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_round_trip() {
        let c = Rgba::from_hex("#1A2B3C").unwrap();
        assert_eq!(c, Rgba::rgb(0x1A, 0x2B, 0x3C));
        assert_eq!(c.to_hex(), "#1A2B3C");
        assert_eq!(Rgba::from_hex("1a2b3c").unwrap(), c);
        assert_eq!(
            Rgba::from_hex("#1A2B3C80").unwrap(),
            Rgba::new(0x1A, 0x2B, 0x3C, 0x80)
        );
    }

    #[test]
    fn hex_rejects_garbage() {
        for bad in ["", "#12345", "#GGGGGG", "0x112233", "#1122334"] {
            assert!(Rgba::from_hex(bad).is_err(), "{bad} は失敗すべき");
        }
    }

    #[test]
    fn blend_opaque_and_transparent() {
        let red = Rgba::rgb(255, 0, 0);
        assert_eq!(red.blend_over(Rgba::WHITE), red);
        assert_eq!(Rgba::TRANSPARENT.blend_over(red), red);
    }

    #[test]
    fn blend_half_alpha_is_midway() {
        let half_white = Rgba::new(255, 255, 255, 128);
        let out = half_white.blend_over(Rgba::BLACK);
        // 128/255 ≒ 0.502 → およそ 128
        assert!((out.r as i32 - 128).abs() <= 1, "実際: {:?}", out);
        assert_eq!(out.r, out.g);
        assert_eq!(out.g, out.b);
        assert_eq!(out.a, 255);
    }

    #[test]
    fn blend_keeps_transparent_when_both_empty() {
        assert_eq!(
            Rgba::TRANSPARENT.blend_over(Rgba::TRANSPARENT),
            Rgba::TRANSPARENT
        );
    }

    #[test]
    fn scaled_alpha_clamps() {
        let c = Rgba::rgb(10, 20, 30);
        assert_eq!(c.scaled_alpha(0.5).a, 128);
        assert_eq!(c.scaled_alpha(2.0).a, 255);
        assert_eq!(c.scaled_alpha(-1.0).a, 0);
    }

    #[test]
    fn palette_is_opaque_and_unique() {
        assert!(PALETTE.iter().all(|c| c.a == 255));
        for (i, a) in PALETTE.iter().enumerate() {
            for b in PALETTE.iter().skip(i + 1) {
                assert_ne!(a, b, "パレットに重複があります");
            }
        }
    }
}
