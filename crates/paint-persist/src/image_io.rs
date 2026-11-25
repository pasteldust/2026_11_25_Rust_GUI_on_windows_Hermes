//! 画像の読み込みと PNG 書き出し。
//!
//! - 読み込みは拡張子に依存せず、`image` クレートの推測で PNG / JPEG / BMP を扱う。
//! - 読み込み結果は必ず RGBA8 に正規化する。
//! - 1 辺が [`MAX_DIMENSION`] を超える画像は [`PersistError::TooLarge`] で拒否する
//!   （ヘッダだけ読んで判定するので、巨大画像でもメモリを食わない）。
//! - 書き出しは [`Document::composite`] の結果を PNG にする。

use std::io::Cursor;
use std::path::Path;

use paint_core::pixel_buffer::MAX_DIMENSION;
use paint_core::{Document, PixelBuffer, Rgba};

use crate::error::{PersistError, Result};

/// PNG 書き出しのオプション。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExportOptions {
    /// 透過を保つか（false なら白で塗ってから書き出す）。
    pub keep_transparency: bool,
}

impl Default for ExportOptions {
    fn default() -> Self {
        Self {
            keep_transparency: true,
        }
    }
}

/// 画像ファイルを読み込んで RGBA8 の画素配列にする。
///
/// 拡張子には依存せず、内容から形式を推測する。
pub fn load_image(path: &Path) -> Result<PixelBuffer> {
    let bytes = std::fs::read(path).map_err(|source| PersistError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    decode(&bytes, Some(path))
}

/// 画像のバイト列を読み込んで RGBA8 の画素配列にする（テスト・自己診断用）。
pub fn load_image_bytes(bytes: &[u8]) -> Result<PixelBuffer> {
    decode(bytes, None)
}

/// 文書を PNG として `path` に書き出す。
///
/// [`ExportOptions::keep_transparency`] が false のときは白で塗ってから書き出す。
pub fn export_png(doc: &Document, path: &Path, options: ExportOptions) -> Result<()> {
    let bytes = export_png_bytes(doc, options)?;
    std::fs::write(path, bytes).map_err(|source| PersistError::Io {
        path: path.to_path_buf(),
        source,
    })
}

/// 文書を PNG のバイト列に変換する（テスト・自己診断用）。
pub fn export_png_bytes(doc: &Document, options: ExportOptions) -> Result<Vec<u8>> {
    let buffer = if options.keep_transparency {
        doc.composite()
    } else {
        doc.composite_on(Rgba::WHITE)
    };
    encode_png(&buffer)
}

/// バイト列をデコードして RGBA8 に正規化する。
fn decode(bytes: &[u8], path: Option<&Path>) -> Result<PixelBuffer> {
    let io_err = |source: std::io::Error| PersistError::Io {
        path: path.map(Path::to_path_buf).unwrap_or_default(),
        source,
    };
    let image_err = |source: image::ImageError| PersistError::Image {
        path: path.map(Path::to_path_buf),
        source,
    };

    // まずヘッダだけ読んで大きさを確認する（巨大画像でメモリを食わない）。
    let (width, height) = image::ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(io_err)?
        .into_dimensions()
        .map_err(image_err)?;
    if width == 0 || height == 0 || width > MAX_DIMENSION || height > MAX_DIMENSION {
        return Err(PersistError::TooLarge {
            width,
            height,
            max: MAX_DIMENSION,
        });
    }

    // 大きさを確認できたので本体をデコードする。
    let decoded = image::ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(io_err)?
        .decode()
        .map_err(image_err)?;
    let rgba = decoded.to_rgba8();
    let (width, height) = rgba.dimensions();
    PixelBuffer::from_rgba8(width, height, rgba.into_raw()).map_err(PersistError::from)
}

/// 画素配列を PNG にエンコードする。
fn encode_png(buffer: &PixelBuffer) -> Result<Vec<u8>> {
    let image =
        image::ImageBuffer::from_raw(buffer.width(), buffer.height(), buffer.data().to_vec())
            .ok_or_else(|| PersistError::Corrupt {
                detail: "画素バッファの大きさが不正です".to_string(),
            })?;
    let mut out = Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(image)
        .write_to(&mut out, image::ImageFormat::Png)
        .map_err(|source| PersistError::Image { path: None, source })?;
    Ok(out.into_inner())
}
