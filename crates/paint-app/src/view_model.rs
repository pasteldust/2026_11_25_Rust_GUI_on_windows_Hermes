//! ViewModel（UI 非依存）。
//!
//! 画面（`app.rs`）はこの型の状態を読んで描き、入力を渡すだけにする。
//! 描画・履歴・ファイル入出力の判断はすべてここに閉じ込めるので、
//! GUI を開かずに一連の操作（読込→描画→保存→再読込→Undo→Redo）をテストできる。
//!
//! ## Undo の持ち方
//!
//! 1 ストローク = 1 コマンド。ストローク中に触る画素の**描く前の色**だけを
//! `originals` に控え、離した時点で「触った範囲の前後の画素」を
//! [`paint_engine::PixelPatch`] にまとめて履歴へ積む。全面スナップショットを
//! 積まないので、大きなキャンバスでも 50 回以上の Undo を現実的なメモリで満たせる。

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use paint_core::{
    Document, Layer, LineWidth, PixelBuffer, Point, Rect, Rgba, Selection, Size, ToolKind,
    ToolSettings,
};
use paint_engine::{
    draw_path, erase_line, erase_rect, line_bounds, stamp_dot, stroke_line, History, HistoryStats,
    PixelPatch, StrokeBuilder,
};
use paint_persist::{export_png, load_image, load_project, save_project, ExportOptions};

/// ズーム倍率の下限。
pub const MIN_ZOOM: f32 = 0.1;
/// ズーム倍率の上限。
pub const MAX_ZOOM: f32 = 16.0;
/// 履歴の既定の保持数（Undo できる回数）。
pub const HISTORY_LIMIT: usize = 100;

/// 描画アプリの ViewModel。
pub struct PainterViewModel {
    document: Document,
    settings: ToolSettings,
    history: History,
    selection: Selection,
    /// 自由線を描いている途中の点列。
    stroke: Option<StrokeBuilder>,
    /// ストローク中に触った画素の、描く前の色。
    originals: HashMap<(i32, i32), Rgba>,
    /// ストロークで触った範囲の合計。
    stroke_rect: Option<Rect>,
    /// 直線モード / 範囲選択の始点。
    drag_start: Option<Point>,
    cursor: Option<Point>,
    zoom: f32,
    offset: (f32, f32),
    status: String,
    dirty: bool,
    path: Option<PathBuf>,
    cache: Option<PixelBuffer>,
    error: Option<String>,
    /// 直線モードで描いているか（Shift+ドラッグ）。
    straight_mode: bool,
    /// 画素が変わった回数（GUI がテクスチャを作り直す判断に使う）。
    revision: u64,
}

impl std::fmt::Debug for PainterViewModel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PainterViewModel")
            .field("canvas", &self.document.canvas_size())
            .field("layers", &self.document.layer_count())
            .field("tool", &self.settings.tool)
            .field("zoom", &self.zoom)
            .field("dirty", &self.dirty)
            .finish_non_exhaustive()
    }
}

impl PainterViewModel {
    /// 指定の大きさのキャンバスで作る（描画レイヤー 1 枚）。
    pub fn new(canvas: Size) -> Self {
        let mut vm = Self {
            document: Document::new(canvas),
            settings: ToolSettings::default(),
            history: History::new(HISTORY_LIMIT),
            selection: Selection::none(),
            stroke: None,
            originals: HashMap::new(),
            stroke_rect: None,
            drag_start: None,
            cursor: None,
            zoom: 1.0,
            offset: (0.0, 0.0),
            status: String::new(),
            dirty: false,
            path: None,
            cache: None,
            error: None,
            straight_mode: false,
            revision: 0,
        };
        vm.refresh_status();
        vm
    }

    /// 読み込んだ文書で作る（テスト用）。
    pub fn with_document(document: Document) -> Self {
        let mut vm = Self::new(document.canvas_size());
        vm.document = document;
        vm.invalidate();
        vm.refresh_status();
        vm
    }

    // ---- 参照 ------------------------------------------------------------

    /// ドキュメント。
    pub fn document(&self) -> &Document {
        &self.document
    }

    /// ツール設定。
    pub fn settings(&self) -> ToolSettings {
        self.settings
    }

    /// 選択範囲。
    pub fn selection(&self) -> Selection {
        self.selection
    }

    /// 履歴の統計。
    pub fn history_stats(&self) -> HistoryStats {
        self.history.stats()
    }

    /// 未保存の変更があるか。
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// 画素が変わった回数。GUI はこれが変わったときだけテクスチャを作り直す。
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// 開いているファイル（未保存なら `None`）。
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    /// ステータスバーに出す文字列。
    pub fn status(&self) -> &str {
        &self.status
    }

    /// 直前のエラー（表示したら [`Self::take_error`] で消す）。
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    /// エラーを取り出す。
    pub fn take_error(&mut self) -> Option<String> {
        self.error.take()
    }

    /// ズーム倍率。
    pub fn zoom(&self) -> f32 {
        self.zoom
    }

    /// 表示の平行移動（画面座標）。
    pub fn offset(&self) -> (f32, f32) {
        self.offset
    }

    /// カーソル位置（キャンバス座標）。
    pub fn cursor(&self) -> Option<Point> {
        self.cursor
    }

    /// ドラッグの始点（プレビュー用）。
    pub fn drag_start(&self) -> Option<Point> {
        self.drag_start
    }

    /// 直線のプレビュー中か（Shift+ドラッグ、または直線ツールでドラッグ中）。
    pub fn is_previewing(&self) -> bool {
        (self.drag_start.is_some() && self.settings.tool == ToolKind::Line) || self.straight_mode
    }

    // ---- 設定 ------------------------------------------------------------

    /// ツールを変える。
    pub fn set_tool(&mut self, tool: ToolKind) {
        self.settings.tool = tool;
        self.refresh_status();
    }

    /// 線幅を変える（レイヤーにも記憶させる）。
    pub fn set_line_width(&mut self, width: LineWidth) {
        self.settings.width = width;
        if let Some(layer) = self.document.active_layer_mut() {
            layer.line_width = width;
        }
        self.refresh_status();
    }

    /// 色を変える（レイヤーにも記憶させる）。
    pub fn set_color(&mut self, color: Rgba) {
        self.settings.color = color;
        if let Some(layer) = self.document.active_layer_mut() {
            layer.color = color;
        }
        self.refresh_status();
    }

    /// ズーム倍率を設定する（範囲外は丸める）。
    pub fn set_zoom(&mut self, zoom: f32) {
        self.zoom = zoom.clamp(MIN_ZOOM, MAX_ZOOM);
        self.refresh_status();
    }

    /// ズーム倍率を掛ける（Ctrl+ホイール）。
    pub fn zoom_by(&mut self, factor: f32) {
        self.set_zoom(self.zoom * factor);
    }

    /// 表示を平行移動する。
    pub fn pan_by(&mut self, dx: f32, dy: f32) {
        self.offset = (self.offset.0 + dx, self.offset.1 + dy);
    }

    /// 表示を初期状態に戻す。
    pub fn reset_view(&mut self) {
        self.zoom = 1.0;
        self.offset = (0.0, 0.0);
        self.refresh_status();
    }

    // ---- 表示用の合成 ----------------------------------------------------

    /// 表示用の合成済み画像（変更があるまでキャッシュ）。
    pub fn composite(&mut self) -> &PixelBuffer {
        if self.cache.is_none() {
            let fresh = self.document.composite();
            self.cache = Some(fresh);
        }
        match self.cache.as_ref() {
            Some(buf) => buf,
            // 上の分岐で必ず入るが、panic を避けて空を返す道も残す
            None => unreachable!("合成キャッシュは直前に作られる"),
        }
    }

    /// 合成キャッシュを捨てる。
    pub fn invalidate(&mut self) {
        self.cache = None;
    }

    // ---- ポインタ入力 ----------------------------------------------------

    /// カーソルが動いた。
    ///
    /// `shift` を押しながら動かすと、ペン／消しゴムはその場で「直線モード」に切り替わる
    /// （要件: シフトを押した状態でマウスを動かすと直線）。直線モードでは確定まで画素を触らない。
    pub fn pointer_move(&mut self, p: Point, shift: bool) {
        self.cursor = Some(p);
        let freehand = matches!(self.settings.tool, ToolKind::Pen | ToolKind::Eraser);
        if freehand && shift && !self.straight_mode {
            self.straight_mode = true;
            if self.drag_start.is_none() {
                self.drag_start = self
                    .stroke
                    .as_ref()
                    .and_then(|builder| builder.start_point())
                    .or(Some(p));
            }
        }
        if freehand && !self.straight_mode && self.stroke.is_some() {
            self.stroke_step_to(p);
        }
        self.refresh_status();
    }

    /// 押した。
    pub fn pointer_down(&mut self, p: Point, shift: bool) {
        self.cursor = Some(p);
        self.selection = Selection::none();
        self.error = None;
        let settings = self.settings;
        match settings.tool {
            ToolKind::Dot => {
                let bounds = line_bounds(p, p, settings.width);
                let color = settings.color;
                let width = settings.width;
                self.draw_once("点", bounds, move |layer| {
                    stamp_dot(layer, p, width, color)
                });
            }
            ToolKind::Pen | ToolKind::Eraser => {
                if shift {
                    // Shift+ドラッグ = 直線。離すまで描かずプレビューだけ出す
                    self.straight_mode = true;
                    self.drag_start = Some(p);
                } else {
                    let mut builder = StrokeBuilder::new(settings);
                    builder.begin(p);
                    self.stroke = Some(builder);
                    self.stroke_step(p, p);
                }
            }
            ToolKind::Line => {
                self.drag_start = Some(p);
            }
            ToolKind::RectSelect => {
                self.drag_start = Some(p);
            }
        }
        self.refresh_status();
    }

    /// 離した。
    pub fn pointer_up(&mut self, p: Point, _shift: bool) {
        self.cursor = Some(p);
        let settings = self.settings;
        match settings.tool {
            ToolKind::Pen | ToolKind::Eraser => {
                if self.straight_mode {
                    self.straight_mode = false;
                    if let Some(start) = self.drag_start.take() {
                        let width = settings.width;
                        let color = settings.color;
                        let eraser = settings.tool == ToolKind::Eraser;
                        let name = if eraser { "直線消去" } else { "直線" };
                        let bounds = line_bounds(start, p, width);
                        self.draw_once(name, bounds, move |layer| {
                            if eraser {
                                erase_line(layer, start, p, width)
                            } else {
                                stroke_line(layer, start, p, width, color)
                            }
                        });
                    }
                } else if self.stroke.is_some() {
                    self.stroke = None;
                    self.stroke_step_to(p);
                    self.commit_stroke(if settings.tool == ToolKind::Eraser {
                        "自由線消去"
                    } else {
                        "自由線"
                    });
                }
            }
            ToolKind::Line => {
                if let Some(start) = self.drag_start.take() {
                    let width = settings.width;
                    let color = settings.color;
                    let bounds = line_bounds(start, p, width);
                    self.draw_once("直線", bounds, move |layer| {
                        stroke_line(layer, start, p, width, color)
                    });
                }
            }
            ToolKind::RectSelect => {
                if let Some(start) = self.drag_start.take() {
                    self.selection = Selection::new(Rect::from_points(start, p));
                }
            }
            ToolKind::Dot => {}
        }
        self.refresh_status();
    }

    /// 選択範囲を消す（Delete キー）。
    pub fn delete_selection(&mut self) {
        let Some(rect) = self.selection.rect else {
            return;
        };
        let canvas = Rect::from_size(self.document.canvas_size());
        let Some(rect) = rect.clamp_to(canvas) else {
            return;
        };
        self.draw_once("範囲消去", rect, move |layer| erase_rect(layer, rect));
        self.selection.clear();
        self.refresh_status();
    }

    /// Undo。
    pub fn undo(&mut self) -> bool {
        let ok = self.history.undo(&mut self.document);
        if ok {
            self.mark_dirty();
        }
        self.refresh_status();
        ok
    }

    /// Redo。
    pub fn redo(&mut self) -> bool {
        let ok = self.history.redo(&mut self.document);
        if ok {
            self.mark_dirty();
        }
        self.refresh_status();
        ok
    }

    // ---- レイヤー操作 ----------------------------------------------------

    /// 描画レイヤーを追加する。
    pub fn add_layer(&mut self) {
        let name = format!("レイヤー {}", self.document.paint_layer_count() + 1);
        self.document.add_paint_layer(name);
        self.mark_dirty();
        self.refresh_status();
    }

    /// 選択中のレイヤーを削除する。
    pub fn remove_active_layer(&mut self) {
        let index = self.document.active_index();
        match self.document.remove_layer(index) {
            Ok(_) => {
                self.history.clear();
                self.mark_dirty();
            }
            Err(e) => self.error = Some(e.to_string()),
        }
        self.refresh_status();
    }

    /// 選択中のレイヤーを前後へ動かす（-1 で背面、+1 で前面）。
    pub fn move_active_layer(&mut self, delta: i32) {
        let from = self.document.active_index();
        let to = from as i32 + delta;
        if to < 0 || to as usize >= self.document.layer_count() {
            return;
        }
        if self.document.move_layer(from, to as usize) {
            self.mark_dirty();
            self.refresh_status();
        }
    }

    /// 選択レイヤーを切り替える（線幅・色もそのレイヤーの記憶に合わせる）。
    pub fn set_active_layer(&mut self, index: usize) {
        if self.document.set_active_index(index) {
            if let Some(layer) = self.document.active_layer() {
                self.settings.width = layer.line_width;
                self.settings.color = layer.color;
            }
            self.refresh_status();
        }
    }

    /// レイヤーの表示を切り替える。
    pub fn toggle_layer_visible(&mut self, index: usize) {
        if let Some(layer) = self.document.layer_mut(index) {
            layer.visible = !layer.visible;
            self.mark_dirty();
            self.refresh_status();
        }
    }

    /// レイヤーの不透明度を変える。
    pub fn set_layer_opacity(&mut self, index: usize, opacity: f32) {
        if let Some(layer) = self.document.layer_mut(index) {
            layer.set_opacity(opacity);
            self.mark_dirty();
            self.refresh_status();
        }
    }

    // ---- ファイル操作 ----------------------------------------------------

    /// 画像を読み込み、下絵レイヤーとして追加する。
    pub fn load_image_file(&mut self, path: &Path) -> Result<(), String> {
        match load_image(path) {
            Ok(pixels) => {
                let name = path
                    .file_name()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_else(|| "画像".to_string());
                match self
                    .document
                    .add_background_layer(format!("下絵: {name}"), pixels)
                {
                    Ok(_) => {
                        self.mark_dirty();
                        self.refresh_status();
                        Ok(())
                    }
                    Err(e) => Err(self.fail(e.to_string())),
                }
            }
            Err(e) => Err(self.fail(e.to_string())),
        }
    }

    /// プロジェクトを保存する。
    pub fn save_project_file(&mut self, path: &Path) -> Result<(), String> {
        match save_project(&self.document, path) {
            Ok(()) => {
                self.path = Some(path.to_path_buf());
                self.dirty = false;
                self.refresh_status();
                Ok(())
            }
            Err(e) => Err(self.fail(e.to_string())),
        }
    }

    /// プロジェクトを開く。
    pub fn open_project_file(&mut self, path: &Path) -> Result<(), String> {
        match load_project(path) {
            Ok(doc) => {
                self.document = doc;
                self.history.clear();
                self.selection.clear();
                self.originals.clear();
                self.stroke_rect = None;
                self.path = Some(path.to_path_buf());
                self.dirty = false;
                if let Some(layer) = self.document.active_layer() {
                    self.settings.width = layer.line_width;
                    self.settings.color = layer.color;
                }
                self.invalidate();
                self.refresh_status();
                Ok(())
            }
            Err(e) => Err(self.fail(e.to_string())),
        }
    }

    /// PNG として書き出す。
    pub fn export_png_file(&mut self, path: &Path, options: ExportOptions) -> Result<(), String> {
        match export_png(&self.document, path, options) {
            Ok(()) => Ok(()),
            Err(e) => Err(self.fail(e.to_string())),
        }
    }

    // ---- 内部 ------------------------------------------------------------

    /// エラーを記録して返す。
    fn fail(&mut self, message: String) -> String {
        self.error = Some(message.clone());
        message
    }

    /// 自由線の 1 点ぶんを描く（前回の点から線でつなぐ）。
    fn stroke_step_to(&mut self, p: Point) {
        let Some(builder) = self.stroke.as_mut() else {
            return;
        };
        let prev = builder.last_point().unwrap_or(p);
        builder.extend(p);
        self.stroke_step(prev, p);
    }

    /// 直線 1 区間を描く（履歴には積まず、元の色だけ控える）。
    fn stroke_step(&mut self, from: Point, to: Point) {
        let settings = self.settings;
        let bounds = line_bounds(from, to, settings.width);
        let eraser = settings.tool == ToolKind::Eraser;
        let width = settings.width;
        let color = settings.color;
        let _ = self.paint(bounds, move |layer| {
            if eraser {
                erase_line(layer, from, to, width)
            } else {
                draw_path(layer, &[from, to], width, color)
            }
        });
    }

    /// 1 回で終わる操作（点・直線・範囲消去）を描いて、その場で履歴に積む。
    fn draw_once<F>(&mut self, name: &str, bounds: Rect, draw: F)
    where
        F: FnOnce(&mut Layer) -> paint_core::Result<Rect>,
    {
        if self.paint(bounds, draw).is_none() {
            self.originals.clear();
            self.stroke_rect = None;
            return;
        }
        self.commit_stroke(name);
    }

    /// 描いて、元の色を控える（履歴には積まない）。
    ///
    /// `bounds` は「この操作が触りうる範囲」。控えるのは描く前なので、
    /// 先に控えてから描く。
    fn paint<F>(&mut self, bounds: Rect, draw: F) -> Option<Rect>
    where
        F: FnOnce(&mut Layer) -> paint_core::Result<Rect>,
    {
        let active = self.document.active_index();
        Self::capture_originals(&self.document, active, bounds, &mut self.originals);
        let affected = {
            let layer = self.document.layer_mut(active)?;
            match draw(layer) {
                Ok(rect) => rect,
                Err(e) => {
                    self.error = Some(e.to_string());
                    return None;
                }
            }
        };
        self.note_stroke_rect(bounds);
        self.note_stroke_rect(affected);
        self.mark_dirty();
        Some(affected)
    }

    /// 描く前に、触りうる範囲の元の色を控える（既に控えた画素は上書きしない）。
    fn capture_originals(
        document: &Document,
        active: usize,
        bounds: Rect,
        originals: &mut HashMap<(i32, i32), Rgba>,
    ) {
        let Some(layer) = document.layer(active) else {
            return;
        };
        let Some(clipped) = bounds.clamp_to(layer.pixels.bounds()) else {
            return;
        };
        for y in clipped.y..(clipped.y + clipped.height as i32) {
            for x in clipped.x..(clipped.x + clipped.width as i32) {
                if originals.contains_key(&(x, y)) {
                    continue;
                }
                if let Some(c) = layer.pixels.get(Point::new(x, y)) {
                    originals.insert((x, y), c);
                }
            }
        }
    }

    /// ストロークの影響範囲を広げる。
    fn note_stroke_rect(&mut self, rect: Rect) {
        if rect.is_empty() {
            return;
        }
        self.stroke_rect = Some(match self.stroke_rect {
            Some(acc) => acc.union(rect),
            None => rect,
        });
    }

    /// ここまでのストロークを 1 コマンドとして履歴に積む。
    fn commit_stroke(&mut self, name: &str) {
        let Some(rect) = self.stroke_rect.take() else {
            self.originals.clear();
            return;
        };
        let active = self.document.active_index();
        let canvas = Rect::from_size(self.document.canvas_size());
        let Some(clipped) = rect.clamp_to(canvas) else {
            self.originals.clear();
            return;
        };
        let Some(layer) = self.document.layer(active) else {
            self.originals.clear();
            return;
        };
        let layer_id = layer.id;

        // after: 現在の画素
        let after = PixelPatch::snapshot(layer, clipped);
        // before: 控えた元の色で置き換える（触っていない画素は現在の値のまま）
        let mut before = after.clone();
        for y in clipped.y..(clipped.y + clipped.height as i32) {
            for x in clipped.x..(clipped.x + clipped.width as i32) {
                if let Some(original) = self.originals.get(&(x, y)) {
                    before.set(Point::new(x - clipped.x, y - clipped.y), *original);
                }
            }
        }

        let patch = PixelPatch::new(layer_id, clipped, before, after);
        let patch = patch_named(patch, name);
        self.history.push(Box::new(patch), &mut self.document);
        self.originals.clear();
        self.mark_dirty();
    }

    /// 変更ありの印を付ける（合成キャッシュも捨てる）。
    fn mark_dirty(&mut self) {
        self.dirty = true;
        self.revision = self.revision.wrapping_add(1);
        self.invalidate();
    }

    /// ステータス文字列を作り直す。
    fn refresh_status(&mut self) {
        let coord = match self.cursor {
            Some(p) => format!("({}, {})", p.x, p.y),
            None => "(-, -)".to_string(),
        };
        let active = self.document.active_index();
        let (name, editable) = self
            .document
            .active_layer()
            .map(|l| {
                (
                    l.name.clone(),
                    if l.is_editable() {
                        ""
                    } else {
                        "（編集不可）"
                    },
                )
            })
            .unwrap_or_else(|| ("-".to_string(), ""));
        let dirty = if self.dirty { "  ●未保存" } else { "" };
        self.status = format!(
            "{coord}  |  ズーム {:.0}%  |  {} {}  |  レイヤー {}: {}{}{}",
            self.zoom * 100.0,
            self.settings.tool.label(),
            self.settings.width,
            active + 1,
            name,
            editable,
            dirty
        );
    }
}

/// 履歴に表示する名前を持つ [`PixelPatch`] の薄い包み。
#[derive(Debug)]
struct NamedPatch {
    inner: PixelPatch,
    name: String,
}

impl paint_engine::Command for NamedPatch {
    fn apply(&mut self, doc: &mut Document) {
        self.inner.apply(doc);
    }
    fn revert(&mut self, doc: &mut Document) {
        self.inner.revert(doc);
    }
    fn name(&self) -> &str {
        &self.name
    }
}

/// 名前を付けたコマンドにする。
fn patch_named(patch: PixelPatch, name: &str) -> NamedPatch {
    NamedPatch {
        inner: patch,
        name: name.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vm(w: u32, h: u32) -> PainterViewModel {
        PainterViewModel::new(Size::new(w, h))
    }

    fn painted(v: &mut PainterViewModel, p: Point) -> bool {
        v.composite().get(p).is_some_and(|c| c.a > 0)
    }

    #[test]
    fn dot_then_undo_redo() {
        let mut v = vm(8, 8);
        v.set_tool(ToolKind::Dot);
        v.set_color(Rgba::rgb(255, 0, 0));
        v.pointer_down(Point::new(3, 3), false);
        v.pointer_up(Point::new(3, 3), false);
        assert_eq!(
            v.composite().get(Point::new(3, 3)),
            Some(Rgba::rgb(255, 0, 0))
        );
        assert_eq!(v.history_stats().undoable, 1);
        assert!(v.undo());
        assert_eq!(v.composite().get(Point::new(3, 3)), Some(Rgba::TRANSPARENT));
        assert!(v.redo());
        assert_eq!(
            v.composite().get(Point::new(3, 3)),
            Some(Rgba::rgb(255, 0, 0))
        );
    }

    #[test]
    fn freehand_stroke_is_one_command_and_undoes_exactly() {
        let mut v = vm(16, 16);
        v.set_tool(ToolKind::Pen);
        v.set_line_width(LineWidth::W2);
        v.pointer_down(Point::new(1, 5), false);
        for x in 2..12 {
            v.pointer_move(Point::new(x, 5), false);
        }
        v.pointer_up(Point::new(12, 5), false);
        assert_eq!(v.history_stats().undoable, 1, "1 ストローク = 1 コマンド");
        assert!(painted(&mut v, Point::new(6, 5)));
        v.undo();
        assert_eq!(
            v.composite().data().iter().filter(|b| **b != 0).count(),
            0,
            "Undo で完全に消える"
        );
    }

    #[test]
    fn shift_drag_draws_straight_line() {
        let mut v = vm(16, 16);
        v.set_tool(ToolKind::Pen);
        v.pointer_down(Point::new(1, 1), true);
        v.pointer_move(Point::new(8, 1), true);
        v.pointer_up(Point::new(14, 14), true);
        assert!(
            painted(&mut v, Point::new(7, 7)),
            "始点と終点を結ぶ線が引かれる"
        );
        assert!(
            !painted(&mut v, Point::new(8, 1)),
            "途中で通っただけの点は塗らない（直線になる）"
        );
        assert_eq!(v.history_stats().undoable, 1);
    }

    #[test]
    fn line_tool_draws_between_press_and_release() {
        let mut v = vm(16, 16);
        v.set_tool(ToolKind::Line);
        v.pointer_down(Point::new(2, 2), false);
        v.pointer_move(Point::new(3, 3), false);
        v.pointer_up(Point::new(12, 2), false);
        assert!(painted(&mut v, Point::new(7, 2)));
        assert!(
            !painted(&mut v, Point::new(3, 3)),
            "ドラッグ中の経由点は描かない"
        );
    }

    #[test]
    fn eraser_clears_alpha_and_rgb() {
        let mut v = vm(8, 8);
        v.set_tool(ToolKind::Dot);
        v.set_line_width(LineWidth::W8);
        v.pointer_down(Point::new(4, 4), false);
        v.pointer_up(Point::new(4, 4), false);
        assert!(painted(&mut v, Point::new(4, 4)));
        v.set_tool(ToolKind::Eraser);
        v.pointer_down(Point::new(4, 4), false);
        v.pointer_up(Point::new(4, 4), false);
        assert_eq!(v.composite().get(Point::new(4, 4)), Some(Rgba::TRANSPARENT));
    }

    #[test]
    fn rect_select_then_delete_clears_area() {
        let mut v = vm(16, 16);
        v.set_tool(ToolKind::Dot);
        v.set_line_width(LineWidth::W16);
        v.pointer_down(Point::new(8, 8), false);
        v.pointer_up(Point::new(8, 8), false);
        assert!(painted(&mut v, Point::new(6, 6)));

        v.set_tool(ToolKind::RectSelect);
        v.pointer_down(Point::new(4, 4), false);
        v.pointer_up(Point::new(12, 12), false);
        assert!(v.selection().is_active());
        v.delete_selection();
        assert!(!painted(&mut v, Point::new(6, 6)));
        assert!(!v.selection().is_active());
    }

    #[test]
    fn background_layer_cannot_be_painted() {
        let mut v = vm(8, 8);
        let img = PixelBuffer::filled(8, 8, Rgba::WHITE);
        v.document.add_background_layer("下絵", img).unwrap();
        v.set_active_layer(0);
        v.set_tool(ToolKind::Dot);
        v.pointer_down(Point::new(1, 1), false);
        v.pointer_up(Point::new(1, 1), false);
        assert_eq!(v.composite().get(Point::new(1, 1)), Some(Rgba::WHITE));
        assert!(v.error().is_some(), "編集不可をエラーとして知らせる");
    }

    #[test]
    fn layer_operations_affect_composite() {
        let mut v = vm(4, 4);
        v.add_layer();
        assert_eq!(v.document().layer_count(), 2);
        v.set_active_layer(1);
        v.set_tool(ToolKind::Dot);
        v.pointer_down(Point::new(0, 0), false);
        v.pointer_up(Point::new(0, 0), false);
        assert!(painted(&mut v, Point::new(0, 0)));
        v.toggle_layer_visible(1);
        assert_eq!(v.composite().get(Point::new(0, 0)), Some(Rgba::TRANSPARENT));
        v.toggle_layer_visible(1);
        v.set_layer_opacity(1, 0.0);
        assert_eq!(v.composite().get(Point::new(0, 0)), Some(Rgba::TRANSPARENT));
    }

    #[test]
    fn zoom_and_pan_are_clamped() {
        let mut v = vm(8, 8);
        v.set_zoom(1000.0);
        assert_eq!(v.zoom(), MAX_ZOOM);
        v.set_zoom(0.0001);
        assert_eq!(v.zoom(), MIN_ZOOM);
        v.zoom_by(2.0);
        assert!(v.zoom() > MIN_ZOOM);
        v.pan_by(10.0, -5.0);
        assert_eq!(v.offset(), (10.0, -5.0));
        v.reset_view();
        assert_eq!(v.offset(), (0.0, 0.0));
        assert_eq!(v.zoom(), 1.0);
    }

    #[test]
    fn status_shows_coordinates_and_dirty_mark() {
        let mut v = vm(8, 8);
        v.pointer_move(Point::new(2, 3), false);
        assert!(v.status().contains("(2, 3)"));
        assert!(!v.is_dirty());
        v.set_tool(ToolKind::Dot);
        v.pointer_down(Point::new(2, 3), false);
        v.pointer_up(Point::new(2, 3), false);
        assert!(v.is_dirty());
        assert!(v.status().contains("未保存"));
    }

    #[test]
    fn fifty_five_strokes_undo_back_to_empty() {
        let mut v = vm(32, 32);
        v.set_tool(ToolKind::Dot);
        for i in 0..55i32 {
            let p = Point::new(i % 32, i / 32);
            v.pointer_down(p, false);
            v.pointer_up(p, false);
        }
        assert_eq!(v.history_stats().undoable, 55);
        for _ in 0..55 {
            assert!(v.undo());
        }
        assert_eq!(v.composite().data().iter().filter(|b| **b != 0).count(), 0);
        assert_eq!(v.history_stats().redoable, 55);
    }

    #[test]
    fn save_and_reopen_keeps_pixels() {
        let dir = std::env::temp_dir().join("paint-vm-test");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("vm.paint");

        let mut v = vm(8, 8);
        v.add_layer();
        v.set_active_layer(1);
        v.set_tool(ToolKind::Dot);
        v.pointer_down(Point::new(2, 2), false);
        v.pointer_up(Point::new(2, 2), false);
        v.save_project_file(&path).unwrap();
        assert!(!v.is_dirty());
        let before = v.document().clone();

        let mut w = vm(4, 4);
        w.open_project_file(&path).unwrap();
        assert_eq!(w.document().canvas_size(), Size::new(8, 8));
        assert_eq!(w.document().layer_count(), before.layer_count());
        assert_eq!(w.composite().get(Point::new(2, 2)), Some(Rgba::BLACK));
        assert_eq!(w.history_stats().undoable, 0, "開いた直後は履歴が空");

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn export_png_keeps_transparency() {
        let dir = std::env::temp_dir().join("paint-vm-test");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("vm.png");
        let mut v = vm(4, 4);
        v.set_tool(ToolKind::Dot);
        v.pointer_down(Point::new(1, 1), false);
        v.pointer_up(Point::new(1, 1), false);
        v.export_png_file(&path, ExportOptions::default()).unwrap();
        assert!(path.exists());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn load_image_adds_locked_background() {
        let dir = std::env::temp_dir().join("paint-vm-test");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("under.png");

        // 小さな PNG を実行時に作る
        let img = image::RgbaImage::from_pixel(4, 4, image::Rgba([10, 20, 30, 255]));
        img.save(&path).unwrap();

        let mut v = vm(4, 4);
        v.load_image_file(&path).unwrap();
        assert!(v.document().has_background());
        assert_eq!(v.document().layer_count(), 2);
        assert_eq!(
            v.composite().get(Point::new(0, 0)),
            Some(Rgba::rgb(10, 20, 30))
        );
        let _ = std::fs::remove_file(&path);
    }
}
