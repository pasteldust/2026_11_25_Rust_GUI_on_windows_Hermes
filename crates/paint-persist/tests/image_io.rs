//! 画像の入出力（読み込み・PNG 書き出し）の統合テスト。
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::io::Cursor;

use paint_core::pixel_buffer::MAX_DIMENSION;
use paint_core::{Document, Point, Rgba, Size};
use paint_persist::{export_png_bytes, load_image_bytes, ExportOptions, PersistError};

/// 1 枚の描画レイヤーを持つ文書を作り、指定の画素だけ不透明にする。
fn doc_with_pixels(size: u32, pixels: &[(i32, i32, Rgba)]) -> Document {
    let mut doc = Document::new(Size::new(size, size));
    for &(x, y, color) in pixels {
        doc.layer_mut(0)
            .unwrap()
            .set_pixel(Point::new(x, y), color)
            .unwrap();
    }
    doc
}

/// バイト列を PNG としてデコードし、`(x, y)` の RGBA を返す。
fn decode_pixel(bytes: &[u8], x: u32, y: u32) -> [u8; 4] {
    let img = image::load_from_memory(bytes).unwrap().to_rgba8();
    let p = img.get_pixel(x, y);
    p.0
}

#[test]
fn export_png_keeps_transparency() {
    // (0,0) は赤で描き、(1,1) は描いてから消しゴムで抜く
    let mut doc = doc_with_pixels(4, &[(0, 0, Rgba::rgb(255, 0, 0))]);
    doc.layer_mut(0)
        .unwrap()
        .set_pixel(Point::new(1, 1), Rgba::rgb(0, 0, 255))
        .unwrap();
    doc.layer_mut(0)
        .unwrap()
        .erase_pixel(Point::new(1, 1))
        .unwrap();

    let bytes = export_png_bytes(&doc, ExportOptions::default()).unwrap();

    assert_eq!(decode_pixel(&bytes, 0, 0), [255, 0, 0, 255]);
    // 消しゴムで抜いた画素はアルファ 0
    assert_eq!(decode_pixel(&bytes, 1, 1), [0, 0, 0, 0]);
    // 何も描いていない画素もアルファ 0
    assert_eq!(decode_pixel(&bytes, 3, 3), [0, 0, 0, 0]);
}

#[test]
fn export_png_fills_white_when_transparency_disabled() {
    let mut doc = doc_with_pixels(4, &[(0, 0, Rgba::rgb(255, 0, 0))]);
    doc.layer_mut(0)
        .unwrap()
        .erase_pixel(Point::new(1, 1))
        .unwrap();

    let options = ExportOptions {
        keep_transparency: false,
    };
    let bytes = export_png_bytes(&doc, options).unwrap();

    // 透明だった部分が白になる
    assert_eq!(decode_pixel(&bytes, 1, 1), [255, 255, 255, 255]);
    assert_eq!(decode_pixel(&bytes, 3, 3), [255, 255, 255, 255]);
    // 描いた画素はそのまま
    assert_eq!(decode_pixel(&bytes, 0, 0), [255, 0, 0, 255]);
}

/// RGBA8 の画像を指定形式でエンコードする。
fn encode_rgba(img: image::RgbaImage, format: image::ImageFormat) -> Vec<u8> {
    let mut out = Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(img)
        .write_to(&mut out, format)
        .unwrap();
    out.into_inner()
}

/// RGB8 の画像を指定形式でエンコードする。
fn encode_rgb(img: image::RgbImage, format: image::ImageFormat) -> Vec<u8> {
    let mut out = Cursor::new(Vec::new());
    image::DynamicImage::ImageRgb8(img)
        .write_to(&mut out, format)
        .unwrap();
    out.into_inner()
}

#[test]
fn load_image_bytes_reads_png_jpeg_and_bmp() {
    // PNG（アルファ付き）
    let mut png = image::RgbaImage::new(3, 2);
    png.put_pixel(0, 0, image::Rgba([255, 0, 0, 255]));
    png.put_pixel(2, 1, image::Rgba([0, 255, 0, 128]));
    let png_bytes = encode_rgba(png, image::ImageFormat::Png);

    // JPEG（アルファ無し）
    let mut jpg = image::RgbImage::new(3, 2);
    jpg.put_pixel(0, 0, image::Rgb([10, 20, 30]));
    let jpg_bytes = encode_rgb(jpg, image::ImageFormat::Jpeg);

    // BMP
    let mut bmp = image::RgbImage::new(3, 2);
    bmp.put_pixel(1, 1, image::Rgb([200, 100, 50]));
    let bmp_bytes = encode_rgb(bmp, image::ImageFormat::Bmp);

    for (name, bytes) in [("PNG", png_bytes), ("JPEG", jpg_bytes), ("BMP", bmp_bytes)] {
        let buffer =
            load_image_bytes(&bytes).unwrap_or_else(|e| panic!("{name} の読込に失敗: {e}"));
        assert_eq!(buffer.size(), Size::new(3, 2), "{name} の大きさ");
        assert_eq!(buffer.data().len(), 3 * 2 * 4, "{name} は RGBA8 に正規化");
    }
}

#[test]
fn load_image_bytes_rejects_broken_data() {
    let broken = b"this is definitely not an image file at all".to_vec();
    assert!(matches!(
        load_image_bytes(&broken),
        Err(PersistError::Image { .. })
    ));
}

/// 大きさだけを持ち、画素データを含まない BMP のヘッダを作る。
///
/// 巨大画像を実際にデコードせずに「大きすぎる画像」を再現するために使う
/// （BMP は DIB ヘッダに大きさが入っており、ヘッダだけで判定できる）。
fn fake_bmp_header(width: u32, height: u32) -> Vec<u8> {
    let mut out = Vec::new();
    // BITMAPFILEHEADER
    out.extend_from_slice(b"BM");
    out.extend_from_slice(&54u32.to_le_bytes()); // bfSize（ヘッダのみ）
    out.extend_from_slice(&0u16.to_le_bytes()); // bfReserved1
    out.extend_from_slice(&0u16.to_le_bytes()); // bfReserved2
    out.extend_from_slice(&54u32.to_le_bytes()); // bfOffBits
                                                 // BITMAPINFOHEADER
    out.extend_from_slice(&40u32.to_le_bytes()); // biSize
    out.extend_from_slice(&(width as i32).to_le_bytes()); // biWidth
    out.extend_from_slice(&(height as i32).to_le_bytes()); // biHeight
    out.extend_from_slice(&1u16.to_le_bytes()); // biPlanes
    out.extend_from_slice(&24u16.to_le_bytes()); // biBitCount
    out.extend_from_slice(&0u32.to_le_bytes()); // biCompression
    out.extend_from_slice(&0u32.to_le_bytes()); // biSizeImage
    out.extend_from_slice(&0i32.to_le_bytes()); // biXPelsPerMeter
    out.extend_from_slice(&0i32.to_le_bytes()); // biYPelsPerMeter
    out.extend_from_slice(&0u32.to_le_bytes()); // biClrUsed
    out.extend_from_slice(&0u32.to_le_bytes()); // biClrImportant
    out
}

#[test]
fn load_image_bytes_rejects_oversized_image() {
    let bytes = fake_bmp_header(MAX_DIMENSION + 1, 10);
    match load_image_bytes(&bytes) {
        Err(PersistError::TooLarge { width, height, max }) => {
            assert_eq!(width, MAX_DIMENSION + 1);
            assert_eq!(height, 10);
            assert_eq!(max, MAX_DIMENSION);
        }
        other => panic!("TooLarge を期待しました: {other:?}"),
    }
}
