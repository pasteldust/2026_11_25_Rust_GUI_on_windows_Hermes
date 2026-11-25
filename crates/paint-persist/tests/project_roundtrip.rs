//! `.paint` 形式の保存・読込の統合テスト。
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::PathBuf;

use paint_core::{Document, LayerKind, LineWidth, PaintError, PixelBuffer, Point, Rgba, Size};
use paint_persist::{
    load_project, load_project_bytes, save_project, save_project_bytes, PersistError,
    PROJECT_MAGIC, PROJECT_VERSION,
};

/// 3 枚（下絵 1 + 描画 2）の文書を組み立てる。
fn three_layer_doc() -> Document {
    let mut doc = Document::new(Size::new(4, 4));
    // 描画レイヤー 1（既定）と 2
    doc.add_paint_layer("レイヤー 2");
    // 下絵を最背面に追加（順序は [下絵, レイヤー1, レイヤー2] になる）
    let bg_pixels = PixelBuffer::filled(4, 4, Rgba::rgb(200, 210, 220));
    doc.add_background_layer("下絵", bg_pixels).unwrap();

    // 下絵（index 0）
    {
        let bg = doc.layer_mut(0).unwrap();
        bg.set_opacity(1.0);
        bg.line_width = LineWidth::W1;
        bg.color = Rgba::BLACK;
    }
    // レイヤー 1（index 1）
    {
        let l = doc.layer_mut(1).unwrap();
        l.set_pixel(Point::new(0, 0), Rgba::rgb(255, 0, 0)).unwrap();
        l.set_pixel(Point::new(3, 3), Rgba::rgb(255, 255, 0))
            .unwrap();
        l.set_opacity(128.0 / 255.0);
        l.line_width = LineWidth::W4;
        l.color = Rgba::rgb(0, 0, 255);
    }
    // レイヤー 2（index 2）は非表示・半透明
    {
        let l = doc.layer_mut(2).unwrap();
        l.set_pixel(Point::new(1, 1), Rgba::rgb(0, 255, 0)).unwrap();
        l.set_opacity(51.0 / 255.0);
        l.line_width = LineWidth::W16;
        l.color = Rgba::rgb(0, 192, 0);
        l.visible = false;
    }
    doc.set_active_index(1);
    doc
}

#[test]
fn round_trip_preserves_layers_metadata_and_pixels() {
    let doc = three_layer_doc();
    let bytes = save_project_bytes(&doc).unwrap();
    let loaded = load_project_bytes(&bytes).unwrap();

    assert_eq!(loaded.canvas_size(), doc.canvas_size());
    assert_eq!(loaded.layer_count(), 3);
    assert_eq!(loaded.paint_layer_count(), 2);
    assert_eq!(loaded.active_index(), doc.active_index());

    for (before, after) in doc.layers().iter().zip(loaded.layers()) {
        assert_eq!(after.name, before.name);
        assert_eq!(after.kind, before.kind);
        assert_eq!(after.visible, before.visible);
        assert_eq!(after.opacity, before.opacity, "不透明度が一致しません");
        assert_eq!(after.line_width, before.line_width);
        assert_eq!(after.color, before.color);
        assert_eq!(
            after.pixels.data(),
            before.pixels.data(),
            "画素が一致しません"
        );
    }

    // 合成結果も一致すること
    assert_eq!(loaded.composite().data(), doc.composite().data());
    // 下絵は最背面のまま
    assert_eq!(loaded.layers()[0].kind, LayerKind::Background);
}

#[test]
fn background_layer_stays_locked_after_round_trip() {
    let doc = three_layer_doc();
    let bytes = save_project_bytes(&doc).unwrap();
    let mut loaded = load_project_bytes(&bytes).unwrap();

    let bg = loaded.layer(0).unwrap();
    assert_eq!(bg.kind, LayerKind::Background);
    assert!(!bg.is_editable());

    assert!(matches!(
        loaded
            .layer_mut(0)
            .unwrap()
            .set_pixel(Point::ORIGIN, Rgba::BLACK),
        Err(PaintError::LayerLocked { .. })
    ));
}

#[test]
fn empty_document_round_trip() {
    let doc = Document::new(Size::new(2, 3));
    let bytes = save_project_bytes(&doc).unwrap();
    let loaded = load_project_bytes(&bytes).unwrap();

    assert_eq!(loaded.canvas_size(), Size::new(2, 3));
    assert_eq!(loaded.layer_count(), 1);
    assert_eq!(loaded.paint_layer_count(), 1);
    assert_eq!(loaded.layers()[0].kind, LayerKind::Paint);
    assert_eq!(loaded.composite().data(), doc.composite().data());
}

#[test]
fn bad_magic_is_rejected() {
    let mut bytes = b"NOTPA".to_vec();
    bytes.extend_from_slice(&[0u8; 32]);
    assert!(matches!(
        load_project_bytes(&bytes),
        Err(PersistError::BadMagic)
    ));
}

#[test]
fn unsupported_version_is_rejected() {
    let mut bytes = save_project_bytes(&three_layer_doc()).unwrap();
    assert_eq!(&bytes[..PROJECT_MAGIC.len()], PROJECT_MAGIC);
    bytes[5..7].copy_from_slice(&99u16.to_le_bytes());
    match load_project_bytes(&bytes) {
        Err(PersistError::UnsupportedVersion { found, supported }) => {
            assert_eq!(found, 99);
            assert_eq!(supported, PROJECT_VERSION);
        }
        other => panic!("UnsupportedVersion を期待しました: {other:?}"),
    }
}

#[test]
fn truncated_file_does_not_panic() {
    let bytes = save_project_bytes(&three_layer_doc()).unwrap();
    for len in [0usize, 3, 5, 7, 10, 23, 40] {
        let cut = &bytes[..len.min(bytes.len())];
        assert!(
            load_project_bytes(cut).is_err(),
            "{len} バイトでエラーになるべき"
        );
    }
}

/// 最初のレイヤーの `data_len` フィールドの位置を求める。
fn first_data_len_offset(bytes: &[u8]) -> usize {
    let mut off = PROJECT_MAGIC.len() + 2 + 4 + 4 + 4 + 4; // magic + version + w + h + count + active
    let name_len = u16::from_le_bytes([bytes[off + 8], bytes[off + 9]]) as usize;
    off += 10 + name_len; // kind/flags/opacity/lw(4) + color(4) + name_len(2) のあと名前
    off
}

#[test]
fn huge_data_len_is_rejected_without_panic() {
    let mut bytes = save_project_bytes(&three_layer_doc()).unwrap();
    let off = first_data_len_offset(&bytes);
    bytes[off..off + 4].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(matches!(
        load_project_bytes(&bytes),
        Err(PersistError::Corrupt { .. })
    ));
}

#[test]
fn missing_end_marker_is_rejected() {
    let mut bytes = save_project_bytes(&three_layer_doc()).unwrap();
    let n = bytes.len();
    bytes[n - 4..].copy_from_slice(b"XXXX");
    assert!(matches!(
        load_project_bytes(&bytes),
        Err(PersistError::Corrupt { .. })
    ));
}

#[test]
fn save_and_load_via_file_path() {
    let doc = three_layer_doc();
    let path: PathBuf = std::env::temp_dir().join(format!(
        "paint_persist_roundtrip_{}.paint",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&path);

    save_project(&doc, &path).unwrap();
    assert!(path.exists());
    // 一時ファイルが残っていないこと
    assert!(!PathBuf::from(format!("{}.tmp", path.display())).exists());

    let loaded = load_project(&path).unwrap();
    assert_eq!(loaded.composite().data(), doc.composite().data());

    std::fs::remove_file(&path).unwrap();
}

#[test]
fn load_missing_file_reports_io_error() {
    let path = std::env::temp_dir().join("paint_persist_definitely_missing_12345.paint");
    let _ = std::fs::remove_file(&path);
    assert!(matches!(load_project(&path), Err(PersistError::Io { .. })));
}
