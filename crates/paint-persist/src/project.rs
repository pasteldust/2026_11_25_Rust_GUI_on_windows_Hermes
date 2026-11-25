//! 独自形式 `.paint` の保存・読込。
//!
//! # ファイル形式（バージョン 1）
//!
//! すべて**リトルエンディアン**。整数の並びは次のとおり。
//!
//! ```text
//! マジック "PAINT"        : 5 バイト
//! version                 : u16  （現在は 1）
//! canvas width            : u32
//! canvas height           : u32
//! layer_count             : u32
//! active_index            : u32
//! --- layer_count 回ぶん繰り返し ---
//!   kind                  : u8   （0 = 下絵, 1 = 描画）
//!   flags                 : u8   （bit0 = visible、他は予約）
//!   opacity               : u8   （0.0〜1.0 の f32 を 0〜255 に量子化）
//!   line_width            : u8   （1, 2, 4, 8, 16, 32 のタグ）
//!   color                 : 4 バイト（RGBA）
//!   name_len              : u16  （UTF-8 のバイト数）
//!   name                  : name_len バイト（UTF-8）
//!   data_len              : u32  （圧縮後のバイト数）
//!   data                  : data_len バイト（zlib 圧縮した RGBA8、長さ = w*h*4）
//! --- ここまで ---
//! 末尾マーカー "END!"     : 4 バイト
//! ```
//!
//! 読み込み側はすべての長さ・範囲を検証し、**どんな壊れた入力でも panic せず
//! [`PersistError`] を返す**。未知のバージョンは説明的なエラーで拒否する。
//!
//! 保存は同じディレクトリの一時ファイル（`*.paint.tmp`）に書いてから `rename`
//! する原子的な手順で行う（書き込み途中のファイルが残らない）。

use std::path::{Path, PathBuf};

use flate2::read::ZlibDecoder;
use flate2::write::ZlibEncoder;
use flate2::Compression;
use std::io::{Read, Write};

use paint_core::pixel_buffer::MAX_DIMENSION;
use paint_core::{Document, Layer, LayerId, LayerKind, LineWidth, PixelBuffer, Rgba, Size};

use crate::error::{PersistError, Result};

/// 拡張子（ドットなし）。
pub const PROJECT_EXTENSION: &str = "paint";
/// ファイル先頭のマジック。
pub const PROJECT_MAGIC: &[u8; 5] = b"PAINT";
/// 現在の形式バージョン。
pub const PROJECT_VERSION: u16 = 1;

/// ファイル末尾のマーカー（切り詰め検出用）。
const END_MARKER: &[u8; 4] = b"END!";

/// 1 レイヤーあたりの固定ヘッダのバイト数
/// （kind + flags + opacity + line_width + color + name_len + data_len）。
const LAYER_HEADER_LEN: usize = 1 + 1 + 1 + 1 + 4 + 2 + 4;

/// 文書を `.paint` 形式で `path` に保存する。
///
/// 同じディレクトリの一時ファイルに書いてから `rename` するため、途中で失敗しても
/// 既存のファイルを壊さない。
pub fn save_project(doc: &Document, path: &Path) -> Result<()> {
    let bytes = save_project_bytes(doc)?;
    let tmp = temp_path(path);
    std::fs::write(&tmp, &bytes).map_err(|source| PersistError::Io {
        path: tmp.clone(),
        source,
    })?;
    std::fs::rename(&tmp, path).map_err(|source| PersistError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    Ok(())
}

/// 文書を `.paint` 形式のバイト列に変換する（テスト・自己診断用）。
pub fn save_project_bytes(doc: &Document) -> Result<Vec<u8>> {
    let size = doc.canvas_size();
    let layers = doc.layers();

    let mut out = Vec::new();
    out.extend_from_slice(PROJECT_MAGIC);
    out.extend_from_slice(&PROJECT_VERSION.to_le_bytes());
    out.extend_from_slice(&size.width.to_le_bytes());
    out.extend_from_slice(&size.height.to_le_bytes());
    out.extend_from_slice(&(layers.len() as u32).to_le_bytes());
    out.extend_from_slice(&(doc.active_index() as u32).to_le_bytes());

    for layer in layers {
        out.push(layer.kind.tag());
        let mut flags = 0u8;
        if layer.visible {
            flags |= 0b0000_0001;
        }
        out.push(flags);
        out.push(quantize_opacity(layer.opacity));
        out.push(layer.line_width.tag());
        out.extend_from_slice(&layer.color.to_array());

        let name = layer.name.as_bytes();
        let name_len = u16::try_from(name.len()).map_err(|_| PersistError::Corrupt {
            detail: format!("レイヤー名が長すぎます（{} バイト）", name.len()),
        })?;
        out.extend_from_slice(&name_len.to_le_bytes());
        out.extend_from_slice(name);

        let compressed = compress(layer.pixels.data())?;
        let data_len = u32::try_from(compressed.len()).map_err(|_| PersistError::Corrupt {
            detail: "圧縮後のデータが大きすぎます".to_string(),
        })?;
        out.extend_from_slice(&data_len.to_le_bytes());
        out.extend_from_slice(&compressed);
    }

    out.extend_from_slice(END_MARKER);
    Ok(out)
}

/// `.paint` ファイルを読み込んで文書を組み立てる。
pub fn load_project(path: &Path) -> Result<Document> {
    let bytes = std::fs::read(path).map_err(|source| PersistError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    load_project_bytes(&bytes)
}

/// `.paint` 形式のバイト列から文書を組み立てる（テスト・自己診断用）。
pub fn load_project_bytes(bytes: &[u8]) -> Result<Document> {
    let mut cur = Cursor::new(bytes);

    let magic = cur.take(PROJECT_MAGIC.len())?;
    if magic != PROJECT_MAGIC {
        return Err(PersistError::BadMagic);
    }

    let version = cur.u16()?;
    if version != PROJECT_VERSION {
        return Err(PersistError::UnsupportedVersion {
            found: version,
            supported: PROJECT_VERSION,
        });
    }

    let width = cur.u32()?;
    let height = cur.u32()?;
    if width == 0 || height == 0 || width > MAX_DIMENSION || height > MAX_DIMENSION {
        return Err(PersistError::Corrupt {
            detail: format!("キャンバスの大きさが不正です: {width}x{height}"),
        });
    }

    let layer_count = cur.u32()? as usize;
    let active = cur.u32()? as usize;

    if layer_count == 0 {
        return Err(PersistError::Corrupt {
            detail: "レイヤーが 1 枚もありません".to_string(),
        });
    }
    // 巨大な layer_count で長いループに入らないよう、残りバイト数から上限を決める。
    if layer_count > cur.remaining() / LAYER_HEADER_LEN {
        return Err(PersistError::Corrupt {
            detail: format!("レイヤー数 {layer_count} がファイルの大きさに対して多すぎます"),
        });
    }

    let pixel_bytes = (width as usize) * (height as usize) * 4;
    let mut layers = Vec::with_capacity(layer_count);
    for index in 0..layer_count {
        let kind_tag = cur.u8()?;
        let kind = LayerKind::from_tag(kind_tag).ok_or_else(|| PersistError::Corrupt {
            detail: format!("レイヤー {index} の種類タグ {kind_tag} が不正です"),
        })?;

        let flags = cur.u8()?;
        let opacity = cur.u8()?;

        let width_tag = cur.u8()?;
        let line_width = LineWidth::from_tag(width_tag).ok_or_else(|| PersistError::Corrupt {
            detail: format!("レイヤー {index} の線幅タグ {width_tag} が不正です"),
        })?;

        let color_bytes = cur.take(4)?;
        let color = Rgba::from_array([
            color_bytes[0],
            color_bytes[1],
            color_bytes[2],
            color_bytes[3],
        ]);

        let name_len = cur.u16()? as usize;
        let name_bytes = cur.take(name_len)?;
        let name = String::from_utf8(name_bytes.to_vec()).map_err(|_| PersistError::Corrupt {
            detail: format!("レイヤー {index} の名前が UTF-8 ではありません"),
        })?;

        let data_len = cur.u32()? as usize;
        let compressed = cur.take(data_len)?;
        let raw = decompress(compressed, pixel_bytes)?;
        let pixels = PixelBuffer::from_rgba8(width, height, raw)?;

        layers.push(Layer {
            id: LayerId(0),
            name,
            kind,
            visible: flags & 0b0000_0001 != 0,
            opacity: f32::from(opacity) / 255.0,
            pixels,
            line_width,
            color,
        });
    }

    let end = cur.take(END_MARKER.len())?;
    if end != END_MARKER {
        return Err(PersistError::Corrupt {
            detail: "末尾マーカー END! がありません".to_string(),
        });
    }

    Ok(Document::from_parts(
        Size::new(width, height),
        layers,
        active,
    )?)
}

/// `opacity`（0.0〜1.0）を 0〜255 に量子化する。
fn quantize_opacity(opacity: f32) -> u8 {
    (opacity.clamp(0.0, 1.0) * 255.0).round() as u8
}

/// `path` に対応する一時ファイルのパス（`*.paint.tmp`）を作る。
fn temp_path(path: &Path) -> PathBuf {
    let mut os = path.as_os_str().to_os_string();
    os.push(".tmp");
    PathBuf::from(os)
}

/// zlib 形式で圧縮する。
fn compress(data: &[u8]) -> Result<Vec<u8>> {
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(data).map_err(|e| PersistError::Corrupt {
        detail: format!("圧縮に失敗しました: {e}"),
    })?;
    encoder.finish().map_err(|e| PersistError::Corrupt {
        detail: format!("圧縮の終了に失敗しました: {e}"),
    })
}

/// zlib 形式を展開する。`expected` バイトと一致しなければ [`PersistError::Corrupt`]。
///
/// 展開結果が `expected` を超えた時点で読み込みを止めるため、圧縮爆弾でも
/// 過大なメモリを確保しない。
fn decompress(data: &[u8], expected: usize) -> Result<Vec<u8>> {
    let decoder = ZlibDecoder::new(data);
    let mut limited = decoder.take(expected as u64 + 1);
    let mut out = Vec::with_capacity(expected.min(1 << 20));
    limited
        .read_to_end(&mut out)
        .map_err(|e| PersistError::Corrupt {
            detail: format!("展開に失敗しました: {e}"),
        })?;
    if out.len() != expected {
        return Err(PersistError::Corrupt {
            detail: format!(
                "展開後の大きさが不正です（期待 {expected}、実際 {}）",
                out.len()
            ),
        });
    }
    Ok(out)
}

/// 境界チェック付きの読み取りカーソル。すべての読み取りは `Result` を返す。
struct Cursor<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Cursor<'a> {
    /// 先頭から読むカーソルを作る。
    fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    /// まだ読めるバイト数。
    fn remaining(&self) -> usize {
        self.data.len().saturating_sub(self.pos)
    }

    /// `n` バイト読み進めて返す。足りなければ [`PersistError::Corrupt`]。
    fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        if self.remaining() < n {
            return Err(PersistError::Corrupt {
                detail: format!(
                    "必要な {n} バイトがありません（残り {} バイト）",
                    self.remaining()
                ),
            });
        }
        let slice = &self.data[self.pos..self.pos + n];
        self.pos += n;
        Ok(slice)
    }

    /// u8 を読む。
    fn u8(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }

    /// u16（リトルエンディアン）を読む。
    fn u16(&mut self) -> Result<u16> {
        let b = self.take(2)?;
        Ok(u16::from_le_bytes([b[0], b[1]]))
    }

    /// u32（リトルエンディアン）を読む。
    fn u32(&mut self) -> Result<u32> {
        let b = self.take(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }
}
