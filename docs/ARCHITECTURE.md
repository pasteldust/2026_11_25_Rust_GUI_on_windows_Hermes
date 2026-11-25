# アーキテクチャ

## 全体像

```
        ┌──────────────────────── paint-app ────────────────────────┐
        │  app.rs (egui 画面)   view_model.rs (UI非依存)  self_test │
        └───────┬───────────────────────┬──────────────────────────┘
                │                       │
      ┌─────────▼──────────┐   ┌────────▼─────────┐
      │   paint-engine     │   │  paint-persist   │
      │ ツール / Undo/Redo │   │ .paint / 画像IO  │
      └─────────┬──────────┘   └────────┬─────────┘
                │                       │
                └───────────┬───────────┘
                    ┌───────▼────────┐
                    │  paint-core    │   ← どこにも依存しない
                    │ ドメイン層     │
                    └────────────────┘
```

依存は必ず**内向き**（app → engine/persist → core）。`paint-core` は外部クレートを
`thiserror` 以外に持たない。GUI もファイル形式も知らないので、テストはヘッドレスで回る。

## クレートの責務

| クレート | 責務 | 依存 |
| --- | --- | --- |
| `paint-core` | 画素（`PixelBuffer`）、レイヤー（`Layer`）、文書（`Document`）、色、幾何、ツール設定値、エラー | `thiserror` のみ |
| `paint-engine` | ラスタライズ（ペン・点・直線・消しゴム・範囲消去）、`Command` と `History`（Undo/Redo） | `paint-core` |
| `paint-persist` | `.paint` の保存・読込、PNG/JPEG/BMP の読込、PNG の書き出し | `paint-core`, `image`, `flate2` |
| `paint-app` | ViewModel（UI 非依存の操作 API とイベント列）、egui/eframe の画面、`--self-test` | すべて |

## なぜこの分け方か

- **描画ロジックを GUI から切り離す**と、描画の正しさをピクセル単位のテストで保証できる。
  egui のウィンドウを開かずに「線が途切れない」「太さが効く」「Undo で元に戻る」を検証する。
- **下絵レイヤーの保護を型で担保する**。`LayerKind::Background` のレイヤーは
  `set_pixel` / `blend_pixel` / `erase_pixel` / `clear` がすべて `Err(LayerLocked)` を返すため、
  UI の作りを間違えても下絵は壊れない。
- **履歴はコマンドの差分**（変更矩形と前後の画素）で持つ。全面スナップショットを積むより
  メモリ効率がよく、50 回以上の Undo を現実的なコストで満たせる。
- **ファイル形式はバージョン付き**。先頭に `PAINT` + バージョンを持ち、未知のバージョンは
  明示的にエラーにする。将来フィールドを増やすときはバージョンを上げて分岐を足す。

## データの流れ（1 ストローク）

```
ポインタ入力（座標 + Shift）
  → view_model: ツールに応じて engine のラスタライズを呼ぶ
  → engine: 変更範囲を計算し、描く前の画素を控えて描画 → 変更矩形と画素を返す
  → view_model: その差分を Command として History に積む
  → app: 合成結果（表示用バッファ）を再描画
```

## 将来機能の置き場所

| 追加したい機能 | 触る場所 | 追加するもの |
| --- | --- | --- |
| レイヤー無制限化 | `paint-core::Document` | 既に上限なし。UI の一覧をスクロール可能に |
| PDF 出力 | `paint-persist` | `export_pdf` を追加（`Document::composite` を再利用） |
| SVG 出力 | `paint-persist` | ベクタ経路を保持する `paint-core` の図形型を追加 |
| 図形描画（矩形・楕円） | `paint-engine::tools` | ラスタライザを追加。`ToolKind` に値を足す |
| テキスト描画 | `paint-engine` | フォントラスタライザ（`ab_glyph` など）を追加 |
| ブラシ機能 | `paint-engine::tools` | `Brush` トレイトを導入し、スタンプ関数を差し替え可能に |
| タブレット筆圧 | `paint-core::ToolSettings`, `paint-app` | 入力に筆圧を足し、`LineWidth` を連続値へ一般化 |
| マクロ | `paint-app` | `Command` 列の記録・再生（履歴の仕組みを再利用） |
| プラグイン | `paint-engine` | `Command` を生成するトレイトを公開して動的読み込み |

## 計測（品質要件）

- 起動時間 3 秒以内・常駐メモリ 500 MB 以下を目標とする。
- 計測手順と実測値は `docs/PERFORMANCE.md` に記録する（E2E ジョブが Windows 上で取得）。
