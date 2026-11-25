//! 画面（eframe/egui）。
//!
//! [`PainterApp`] は [`crate::view_model::PainterViewModel`] をそのまま抱き、
//! 状態を読んで描き、入力を渡すだけの薄い層。描画・履歴・ファイル入出力の
//! 判断はすべて ViewModel 側にある。
//!
//! ## 画面構成
//!
//! - **左 SidePanel**: ツールパレット（ツール・線幅・色・RGB スライダ・Undo/Redo）
//! - **右 SidePanel**: レイヤーパネル（前面が上。表示・不透明度・並べ替え）
//! - **上 TopBottomPanel**: ファイル名と未保存マーク
//! - **下 TopBottomPanel**: ステータスバー（座標・ズーム・エラー・未保存）
//! - **中央 CentralPanel**: キャンバス（テクスチャ・市松模様・プレビュー）
//!
//! ## ショートカット
//!
//! - `Ctrl+O` — 画像を読み込み、下絵レイヤーとして追加する
//! - `Ctrl+Shift+O` — **プロジェクト（`.paint`）を開く**
//! - `Ctrl+S` — プロジェクトとして保存（`.paint`）
//! - `Ctrl+Shift+S` — PNG として書き出し
//! - `Ctrl+Z` — Undo
//! - `Ctrl+Y` — Redo
//! - `Delete` — 選択範囲を消す
//!
//! `Ctrl+O` は画像の読み込みに割り当てているため、プロジェクトを開くのは
//! `Ctrl+Shift+O` にしてある。

use eframe::egui;
use egui::{Color32, ColorImage, Sense, Stroke, StrokeKind, TextureHandle, TextureOptions};

use paint_core::color::PALETTE;
use paint_core::{LineWidth, Point, Rgba, ToolKind};
use paint_persist::{ExportOptions, PROJECT_EXTENSION};

use crate::view_model::PainterViewModel;

/// 透明部分を示す市松模様の 1 マスの大きさ（画面 px）。
const CHECKER_TILE: f32 = 16.0;

/// 画面の状態。ViewModel と、GUI だけが持つ表示用のキャッシュを束ねる。
pub struct PainterApp {
    /// 描画アプリの状態（UI 非依存）。
    view_model: PainterViewModel,
    /// キャンバス表示用のテクスチャ（`revision` が変わったときだけ作り直す）。
    texture: Option<TextureHandle>,
    /// テクスチャを作ったときの `revision`。
    texture_revision: u64,
    /// いま左ボタンでドラッグ中か（`pointer_down` を 1 回だけ呼ぶため）。
    dragging: bool,
    /// 直近のエラー（`take_error` で取り出して表示する）。
    error: Option<String>,
}

impl std::fmt::Debug for PainterApp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PainterApp")
            .field("view_model", &self.view_model)
            .field("dragging", &self.dragging)
            .finish_non_exhaustive()
    }
}

impl PainterApp {
    /// ViewModel を受け取って画面を作る。
    pub fn new(view_model: PainterViewModel) -> Self {
        Self {
            view_model,
            texture: None,
            // 最初のフレームで必ず作り直すよう、実在しない値にしておく。
            texture_revision: u64::MAX,
            dragging: false,
            error: None,
        }
    }

    /// ファイル名（無題なら「新規」）を返す。
    fn display_name(&self) -> String {
        match self.view_model.path().and_then(|p| p.file_name()) {
            Some(name) => name.to_string_lossy().to_string(),
            None => "新規".to_string(),
        }
    }

    /// プロジェクトを保存する（`Ctrl+S`）。
    fn save_project(&mut self) {
        let mut dialog =
            rfd::FileDialog::new().add_filter("お絵描きプロジェクト", &[PROJECT_EXTENSION]);
        match self.view_model.path().and_then(|p| p.file_name()) {
            Some(name) => {
                dialog = dialog.set_file_name(name.to_string_lossy().to_string());
            }
            None => {
                dialog = dialog.set_file_name(format!("無題.{PROJECT_EXTENSION}"));
            }
        }
        if let Some(mut path) = dialog.save_file() {
            if path.extension().is_none() {
                path.set_extension(PROJECT_EXTENSION);
            }
            if let Err(message) = self.view_model.save_project_file(&path) {
                self.error = Some(message);
            }
        }
    }

    /// PNG として書き出す（`Ctrl+Shift+S`）。
    fn export_png(&mut self) {
        let dialog = rfd::FileDialog::new()
            .add_filter("PNG 画像", &["png"])
            .set_file_name("書き出し.png");
        if let Some(mut path) = dialog.save_file() {
            if path.extension().is_none() {
                path.set_extension("png");
            }
            if let Err(message) = self
                .view_model
                .export_png_file(&path, ExportOptions::default())
            {
                self.error = Some(message);
            }
        }
    }

    /// 画像を読み込んで下絵にする（`Ctrl+O`）。
    fn load_image(&mut self) {
        let dialog = rfd::FileDialog::new().add_filter("画像", &["png", "jpg", "jpeg", "bmp"]);
        if let Some(path) = dialog.pick_file() {
            if let Err(message) = self.view_model.load_image_file(&path) {
                self.error = Some(message);
            }
        }
    }

    /// プロジェクトを開く（`Ctrl+Shift+O`）。
    fn open_project(&mut self) {
        let dialog =
            rfd::FileDialog::new().add_filter("お絵描きプロジェクト", &[PROJECT_EXTENSION]);
        if let Some(path) = dialog.pick_file() {
            if let Err(message) = self.view_model.open_project_file(&path) {
                self.error = Some(message);
            }
        }
    }

    /// キーボードショートカットを処理する。
    fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        let ctrl = egui::Modifiers::CTRL;
        let ctrl_shift = egui::Modifiers::CTRL | egui::Modifiers::SHIFT;

        // Shift 付きを先に消費する（Ctrl+O と取り違えないため）。
        let open_project = ctx.input_mut(|i| {
            i.consume_shortcut(&egui::KeyboardShortcut::new(ctrl_shift, egui::Key::O))
        });
        let export_png = ctx.input_mut(|i| {
            i.consume_shortcut(&egui::KeyboardShortcut::new(ctrl_shift, egui::Key::S))
        });
        let open_image =
            ctx.input_mut(|i| i.consume_shortcut(&egui::KeyboardShortcut::new(ctrl, egui::Key::O)));
        let save_project =
            ctx.input_mut(|i| i.consume_shortcut(&egui::KeyboardShortcut::new(ctrl, egui::Key::S)));
        let undo =
            ctx.input_mut(|i| i.consume_shortcut(&egui::KeyboardShortcut::new(ctrl, egui::Key::Z)));
        let redo =
            ctx.input_mut(|i| i.consume_shortcut(&egui::KeyboardShortcut::new(ctrl, egui::Key::Y)));
        let delete = ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Delete));

        if open_project {
            self.open_project();
        }
        if export_png {
            self.export_png();
        }
        if open_image {
            self.load_image();
        }
        if save_project {
            self.save_project();
        }
        if undo {
            self.view_model.undo();
        }
        if redo {
            self.view_model.redo();
        }
        if delete {
            self.view_model.delete_selection();
        }
    }

    /// 左のツールパレットを描く。
    fn tools_panel(&mut self, ui: &mut egui::Ui) {
        ui.heading("ツール");
        ui.separator();

        let current_tool = self.view_model.settings().tool;
        for tool in ToolKind::all() {
            if ui
                .selectable_label(tool == current_tool, tool.label())
                .clicked()
            {
                self.view_model.set_tool(tool);
            }
        }

        ui.add_space(8.0);
        ui.heading("線幅");
        let current_width = self.view_model.settings().width;
        ui.horizontal_wrapped(|ui| {
            for width in LineWidth::all() {
                if ui
                    .selectable_label(width == current_width, width.to_string())
                    .clicked()
                {
                    self.view_model.set_line_width(width);
                }
            }
        });

        ui.add_space(8.0);
        ui.heading("色");
        let current_color = self.view_model.settings().color;
        let mut picked: Option<Rgba> = None;
        ui.horizontal_wrapped(|ui| {
            for color in PALETTE {
                let (rect, response) =
                    ui.allocate_exact_size(egui::vec2(20.0, 20.0), Sense::click());
                let fill = to_color32(color);
                ui.painter().rect_filled(rect, 2.0, fill);
                if color == current_color {
                    ui.painter().rect_stroke(
                        rect,
                        2.0,
                        Stroke::new(2.0, Color32::WHITE),
                        StrokeKind::Inside,
                    );
                    ui.painter().rect_stroke(
                        rect,
                        2.0,
                        Stroke::new(1.0, Color32::BLACK),
                        StrokeKind::Outside,
                    );
                }
                if response.clicked() {
                    picked = Some(color);
                }
            }
        });
        if let Some(color) = picked {
            self.view_model.set_color(color);
        }

        // RGB（+ アルファ）スライダ
        let mut edited = current_color;
        let mut changed = false;
        ui.horizontal(|ui| {
            changed |= ui
                .add(egui::Slider::new(&mut edited.r, 0..=255).text("R"))
                .changed();
        });
        ui.horizontal(|ui| {
            changed |= ui
                .add(egui::Slider::new(&mut edited.g, 0..=255).text("G"))
                .changed();
        });
        ui.horizontal(|ui| {
            changed |= ui
                .add(egui::Slider::new(&mut edited.b, 0..=255).text("B"))
                .changed();
        });
        ui.horizontal(|ui| {
            changed |= ui
                .add(egui::Slider::new(&mut edited.a, 0..=255).text("A"))
                .changed();
        });
        if changed {
            self.view_model.set_color(edited);
        }

        ui.add_space(8.0);
        ui.separator();
        let stats = self.view_model.history_stats();
        ui.horizontal(|ui| {
            let undo = ui.add_enabled(
                stats.undoable > 0,
                egui::Button::new(format!("Undo ({})", stats.undoable)),
            );
            if undo.clicked() {
                self.view_model.undo();
            }
            let redo = ui.add_enabled(
                stats.redoable > 0,
                egui::Button::new(format!("Redo ({})", stats.redoable)),
            );
            if redo.clicked() {
                self.view_model.redo();
            }
        });

        ui.add_space(8.0);
        if ui.button("表示をリセット").clicked() {
            self.view_model.reset_view();
        }
    }

    /// 右のレイヤーパネルを描く。
    fn layers_panel(&mut self, ui: &mut egui::Ui) {
        ui.heading("レイヤー");
        ui.separator();

        // 借用を分けるため、先に一覧をコピーする。
        let active = self.view_model.document().active_index();
        let rows: Vec<LayerRow> = self
            .view_model
            .document()
            .layers()
            .iter()
            .enumerate()
            .map(|(index, layer)| LayerRow {
                index,
                name: layer.name.clone(),
                visible: layer.visible,
                opacity: layer.opacity,
                editable: layer.is_editable(),
                active: index == active,
            })
            .collect();

        let mut select: Option<usize> = None;
        let mut toggle: Option<usize> = None;
        let mut opacity: Option<(usize, f32)> = None;

        // 前面が上に見えるよう逆順に並べる。
        egui::ScrollArea::vertical()
            .max_height(ui.available_height() - 40.0)
            .show(ui, |ui| {
                for row in rows.iter().rev() {
                    egui::Frame::group(ui.style()).show(ui, |ui| {
                        ui.horizontal(|ui| {
                            if ui.selectable_label(row.active, row.name.as_str()).clicked() {
                                select = Some(row.index);
                            }
                            if !row.editable {
                                ui.weak("編集不可");
                            }
                        });
                        ui.horizontal(|ui| {
                            let mut visible = row.visible;
                            if ui.checkbox(&mut visible, "表示").changed() {
                                toggle = Some(row.index);
                            }
                        });
                        ui.horizontal(|ui| {
                            let mut value = row.opacity;
                            if ui
                                .add(egui::Slider::new(&mut value, 0.0..=1.0).text("不透明度"))
                                .changed()
                            {
                                opacity = Some((row.index, value));
                            }
                        });
                    });
                }
            });

        if let Some(index) = select {
            self.view_model.set_active_layer(index);
        }
        if let Some(index) = toggle {
            self.view_model.toggle_layer_visible(index);
        }
        if let Some((index, value)) = opacity {
            self.view_model.set_layer_opacity(index, value);
        }

        ui.separator();
        ui.horizontal(|ui| {
            if ui.button("追加").clicked() {
                self.view_model.add_layer();
            }
            if ui.button("削除").clicked() {
                self.view_model.remove_active_layer();
            }
        });
        ui.horizontal(|ui| {
            if ui.button("上へ").clicked() {
                self.view_model.move_active_layer(1);
            }
            if ui.button("下へ").clicked() {
                self.view_model.move_active_layer(-1);
            }
        });
    }

    /// 中央のキャンバスを描き、ポインタ入力を渡す。
    fn canvas_panel(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let (response, painter) = ui.allocate_painter(ui.available_size(), Sense::click_and_drag());
        let viewport = response.rect;

        let zoom = self.view_model.zoom();
        let (offset_x, offset_y) = self.view_model.offset();
        let canvas = self.view_model.document().canvas_size();
        let origin = egui::pos2(viewport.min.x + offset_x, viewport.min.y + offset_y);
        let image_rect = egui::Rect::from_min_size(
            origin,
            egui::vec2(canvas.width as f32 * zoom, canvas.height as f32 * zoom),
        );

        // 暗い背景。
        painter.rect_filled(viewport, 0.0, Color32::from_gray(32));

        // 透明が分かる市松模様（画像の下に敷く）。
        draw_checkerboard(&painter, image_rect);

        // テクスチャ（revision が変わったときだけ作り直す）。
        self.ensure_texture(ctx);
        if let Some(texture) = &self.texture {
            let uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
            painter.image(texture.id(), image_rect, uv, Color32::WHITE);
        }

        // キャンバスの外枠。
        painter.rect_stroke(
            image_rect,
            0.0,
            Stroke::new(1.0, Color32::from_gray(90)),
            StrokeKind::Inside,
        );

        // ---- 入力 ---------------------------------------------------------
        let (
            primary_down,
            middle_down,
            secondary_down,
            pointer_pos,
            shift,
            scroll,
            zoom_delta,
            ctrl,
            pan_delta,
        ) = ui.input(|i| {
            (
                i.pointer.primary_down(),
                i.pointer.middle_down(),
                i.pointer.secondary_down(),
                i.pointer.latest_pos(),
                i.modifiers.shift,
                i.smooth_scroll_delta,
                i.zoom_delta(),
                i.modifiers.command || i.modifiers.ctrl,
                i.pointer.delta(),
            )
        });

        let to_canvas = |pos: egui::Pos2| -> Point {
            let x = ((pos.x - origin.x) / zoom).floor();
            let y = ((pos.y - origin.y) / zoom).floor();
            Point::new(x as i32, y as i32)
        };
        let to_screen = |point: Point| -> egui::Pos2 {
            egui::pos2(
                origin.x + point.x as f32 * zoom,
                origin.y + point.y as f32 * zoom,
            )
        };

        // 中ボタン / 右ボタンのドラッグで平行移動。
        if (middle_down || secondary_down) && response.hovered() {
            self.view_model.pan_by(pan_delta.x, pan_delta.y);
        }

        // ホイール: Ctrl 付きはズーム、それ以外は縦スクロール。
        if response.hovered() {
            if ctrl {
                let factor = if (zoom_delta - 1.0).abs() > 1.0e-4 {
                    zoom_delta
                } else if scroll.y.abs() > 0.0 {
                    (scroll.y * 0.0015).exp()
                } else {
                    1.0
                };
                if (factor - 1.0).abs() > 1.0e-6 {
                    self.view_model.zoom_by(factor);
                }
            } else if scroll != egui::Vec2::ZERO {
                self.view_model.pan_by(scroll.x, scroll.y);
            }
        }

        // 左ボタンの描画。
        if let Some(pos) = pointer_pos {
            let canvas_point = to_canvas(pos);
            if primary_down {
                if !self.dragging && response.hovered() {
                    self.dragging = true;
                    self.view_model.pointer_down(canvas_point, shift);
                } else if self.dragging {
                    self.view_model.pointer_move(canvas_point, shift);
                }
            } else if self.dragging {
                self.dragging = false;
                self.view_model.pointer_up(canvas_point, shift);
            } else {
                self.view_model.pointer_move(canvas_point, shift);
            }
        }

        // ---- プレビュー（確定前は画素を触らない）--------------------------
        let preview_color = to_color32(self.view_model.settings().color);
        let preview_stroke = Stroke::new(2.0, preview_color);

        if self.view_model.is_previewing() {
            if let (Some(start), Some(end)) =
                (self.view_model.drag_start(), self.view_model.cursor())
            {
                painter.line_segment([to_screen(start), to_screen(end)], preview_stroke);
            }
        }

        // 範囲選択（確定済み）。
        if let Some(rect) = self.view_model.selection().rect {
            painter.rect_stroke(
                canvas_rect_to_screen(&to_screen, rect),
                0.0,
                Stroke::new(2.0, Color32::from_rgb(255, 220, 0)),
                StrokeKind::Inside,
            );
        }

        // 範囲選択（ドラッグ中）。
        if self.view_model.settings().tool == ToolKind::RectSelect {
            if let (Some(start), Some(end)) =
                (self.view_model.drag_start(), self.view_model.cursor())
            {
                let rect = paint_core::Rect::from_points(start, end);
                painter.rect_stroke(
                    canvas_rect_to_screen(&to_screen, rect),
                    0.0,
                    Stroke::new(1.0, Color32::from_rgb(255, 220, 0)),
                    StrokeKind::Inside,
                );
            }
        }
    }

    /// キャンバスのテクスチャを、必要なら作り直す。
    fn ensure_texture(&mut self, ctx: &egui::Context) {
        let revision = self.view_model.revision();
        if self.texture.is_some() && revision == self.texture_revision {
            return;
        }
        let buffer = self.view_model.composite();
        let (width, height) = (buffer.width(), buffer.height());
        if width == 0 || height == 0 {
            return;
        }
        let image =
            ColorImage::from_rgba_unmultiplied([width as usize, height as usize], buffer.data());
        match &mut self.texture {
            Some(texture) => texture.set(image, TextureOptions::NEAREST),
            None => {
                self.texture = Some(ctx.load_texture("canvas", image, TextureOptions::NEAREST));
            }
        }
        self.texture_revision = revision;
    }
}

/// レイヤー 1 行分の表示用データ（借用を分けるためのコピー）。
struct LayerRow {
    index: usize,
    name: String,
    visible: bool,
    opacity: f32,
    editable: bool,
    active: bool,
}

impl eframe::App for PainterApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        // ルートの Ui から Context を取り出す（ショートカットとテクスチャ作成に使う）。
        let ctx = ui.ctx().clone();

        // ViewModel が持つエラーを取り出して保持する（ステータスバーに出す）。
        if let Some(message) = self.view_model.take_error() {
            self.error = Some(message);
        }

        self.handle_shortcuts(&ctx);

        // 上: ファイル名と未保存マーク。
        egui::Panel::top("title").show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.strong(self.display_name());
                if self.view_model.is_dirty() {
                    ui.colored_label(Color32::from_rgb(220, 80, 80), "●未保存");
                }
            });
        });

        // 下: ステータスバー（座標・ズーム・ツール・レイヤーは ViewModel が出す）。
        egui::Panel::bottom("status").show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(self.view_model.status());
                if let Some(message) = &self.error {
                    ui.separator();
                    ui.colored_label(Color32::from_rgb(230, 70, 70), format!("エラー: {message}"));
                }
            });
        });

        // 左: ツール。
        egui::Panel::left("tools")
            .resizable(true)
            .default_size(200.0)
            .show(ui, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    self.tools_panel(ui);
                });
            });

        // 右: レイヤー。
        egui::Panel::right("layers")
            .resizable(true)
            .default_size(240.0)
            .show(ui, |ui| {
                self.layers_panel(ui);
            });

        // 中央: キャンバス。
        egui::CentralPanel::default().show(ui, |ui| {
            self.canvas_panel(ui, &ctx);
        });
    }
}

/// キャンバス座標の矩形を画面座標の矩形へ直す。
fn canvas_rect_to_screen(
    to_screen: &impl Fn(Point) -> egui::Pos2,
    rect: paint_core::Rect,
) -> egui::Rect {
    let min = to_screen(Point::new(rect.x, rect.y));
    let max = to_screen(Point::new(
        rect.x + rect.width as i32,
        rect.y + rect.height as i32,
    ));
    egui::Rect::from_min_max(min, max)
}

/// 透明部分が分かるよう、画像の領域に市松模様を敷く。
fn draw_checkerboard(painter: &egui::Painter, image_rect: egui::Rect) {
    let visible = image_rect.intersect(painter.clip_rect());
    if visible.width() <= 0.0 || visible.height() <= 0.0 {
        return;
    }
    let tile = CHECKER_TILE;
    let x0 = ((visible.left() - image_rect.left()) / tile).floor() as i32;
    let y0 = ((visible.top() - image_rect.top()) / tile).floor() as i32;
    let x1 = ((visible.right() - image_rect.left()) / tile).ceil() as i32;
    let y1 = ((visible.bottom() - image_rect.top()) / tile).ceil() as i32;
    for ty in y0..y1 {
        for tx in x0..x1 {
            let color = if (tx + ty) % 2 == 0 {
                Color32::from_gray(120)
            } else {
                Color32::from_gray(165)
            };
            let rect = egui::Rect::from_min_size(
                egui::pos2(
                    image_rect.left() + tx as f32 * tile,
                    image_rect.top() + ty as f32 * tile,
                ),
                egui::vec2(tile, tile),
            );
            painter.rect_filled(rect.intersect(visible), 0.0, color);
        }
    }
}

/// [`Rgba`] を egui の [`Color32`] へ。
fn to_color32(color: Rgba) -> Color32 {
    Color32::from_rgba_unmultiplied(color.r, color.g, color.b, color.a)
}

// ---- 日本語フォント ------------------------------------------------------

/// 日本語を表示するためのフォント候補（OS ごと）。
///
/// egui に同梱されているフォントは日本語の字形を持たないため、画面の日本語が
/// 豆腐（□）になってしまう。実行時に OS のフォントを借りて解決する。
pub fn japanese_font_candidates() -> Vec<String> {
    let windows = std::env::var("WINDIR").unwrap_or_else(|_| r"C:\Windows".to_string());
    vec![
        // Windows 10 / 11
        format!(r"{windows}\Fonts\YuGothM.ttc"),
        format!(r"{windows}\Fonts\meiryo.ttc"),
        format!(r"{windows}\Fonts\msgothic.ttc"),
        format!(r"{windows}\Fonts\msmincho.ttc"),
        // Linux
        "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc".to_string(),
        "/usr/share/fonts/truetype/noto/NotoSansCJK-Regular.ttc".to_string(),
        "/usr/share/fonts/opentype/ipafont-gothic/ipag.ttf".to_string(),
        "/usr/share/fonts/truetype/fonts-japanese-gothic.ttf".to_string(),
        // macOS（配布対象ではないが、開発機で見られると便利）
        "/System/Library/Fonts/ヒラギノ角ゴシック W4.ttc".to_string(),
        "/System/Library/Fonts/Hiragino Sans GB.ttc".to_string(),
    ]
}

/// 存在する最初の候補を選ぶ（無ければ `None`）。
pub fn pick_font_path(candidates: &[String], exists: impl Fn(&str) -> bool) -> Option<String> {
    candidates.iter().find(|path| exists(path)).cloned()
}

/// OS の日本語フォントを egui に登録し、読み込んだパスを返す。
///
/// 見つからなくても起動は続ける（日本語だけが豆腐になる）。
pub fn install_japanese_font(ctx: &egui::Context) -> Option<String> {
    let path = pick_font_path(&japanese_font_candidates(), |p| {
        std::path::Path::new(p).is_file()
    })?;
    let bytes = std::fs::read(&path).ok()?;
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert(
        "japanese_os".to_owned(),
        std::sync::Arc::new(egui::FontData::from_owned(bytes)),
    );
    // 既定フォントの後ろに足す（英数字は今までどおり、日本語だけ補う）
    for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
        fonts
            .families
            .entry(family)
            .or_default()
            .push("japanese_os".to_owned());
    }
    ctx.set_fonts(fonts);
    Some(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pick_font_path_returns_first_existing() {
        let candidates = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        assert_eq!(
            pick_font_path(&candidates, |p| p == "b").as_deref(),
            Some("b")
        );
    }

    #[test]
    fn pick_font_path_returns_none_when_nothing_exists() {
        let candidates = vec!["x".to_string(), "y".to_string()];
        assert_eq!(pick_font_path(&candidates, |_| false), None);
    }

    #[test]
    fn candidates_cover_windows_and_linux() {
        let list = japanese_font_candidates();
        assert!(
            list.iter()
                .any(|p| p.contains("YuGothM") || p.contains("meiryo")),
            "Windows の候補が無い"
        );
        assert!(
            list.iter().any(|p| p.contains("NotoSansCJK")),
            "Linux の候補が無い"
        );
    }
}
