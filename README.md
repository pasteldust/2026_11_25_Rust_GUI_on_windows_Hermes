# Rust GUI お絵描きソフト（Windows）

Windows 10/11 (64bit) で単体動作する、Rust 製のお絵描きソフトです。
画像を下絵として読み込み、その上にレイヤーを重ねて描き、独自形式（`.paint`）で保存できます。

- GUI: **egui / eframe**（純 Rust。ネイティブの exe として動作します）
- 描画エンジン・永続化・UI をクレートで分離し、GUI を開かずにテストできる構造
- CI（GitHub Actions）で `fmt` / `clippy` / `test` / `release build` と Windows 上の E2E を自動実行

## 動作環境

| 項目 | 内容 |
| --- | --- |
| OS | Windows 10 / Windows 11（64bit） |
| ビルド | Rust stable（1.85 以上） |
| 出力 | 単体で動く exe（`target/release/paint-app.exe`） |

## ビルド方法

```powershell
# Rust の導入（未導入なら）
winget install --id Rustlang.Rustup

git clone https://github.com/pasteldust/2026_11_25_Rust_GUI_on_windows_Hermes.git
cd 2026_11_25_Rust_GUI_on_windows_Hermes

cargo build --release
```

## 実行方法

```powershell
cargo run --release
# もしくはビルド済みの exe を直接
.\target\release\paint-app.exe
```

GUI を開かずに一連の操作（起動・描画・保存・再読込・Undo・Redo）を確認する自己テストもあります
（CI の E2E でもこれを使います）。

```powershell
.\target\release\paint-app.exe --self-test .\out
```

## 画面

```
┌──────────────┬────────────────────────────────┬──────────────┐
│ ツール        │                                │ レイヤー      │
│ パレット      │          キャンバス             │ パネル        │
│              │                                │              │
│ ペン / 点     │   （下絵 + レイヤーを合成表示）   │ 下絵（編集不可）│
│ 直線 / 消しゴム│                                │ レイヤー 1     │
│ 範囲消去      │                                │ 追加 / 削除 / │
│              │                                │ 並べ替え      │
│ 線幅 1〜32px  │                                │              │
│ カラーパレット │                                │              │
├──────────────┴────────────────────────────────┴──────────────┤
│ ステータスバー: 座標 / ズーム / ツール / レイヤー / 未保存マーク   │
└──────────────────────────────────────────────────────────────┘
```

- **左**: ツールパレット（ツール、線幅 1/2/4/8/16/32px、カラーパレット）
- **中央**: キャンバス。`Ctrl+ホイール`でズーム、スクロールで移動
- **右**: レイヤーパネル（表示切替、不透明度、追加・削除・並べ替え。下絵は編集不可と表示）
- **下**: ステータスバー

## ショートカット

| キー | 動作 |
| --- | --- |
| `Ctrl+O` | 画像を読み込む（PNG / JPEG / BMP。下絵レイヤーとして追加） |
| `Ctrl+S` | プロジェクトを保存（`.paint`） |
| `Ctrl+Shift+S` | PNG として書き出す（透過を保持） |
| `Ctrl+Z` | Undo（50 回以上） |
| `Ctrl+Y` | Redo（50 回以上） |
| `Delete` | 選択範囲（範囲消去ツールで指定した矩形）を消す |
| `Ctrl+ホイール` | ズーム |
| `Shift` + ドラッグ | 直線を引く（直線ツール） |

## 主な機能

- **下絵**: PNG / JPEG / BMP を読み込み、編集できない下絵レイヤーとして背景に表示
- **描画**: ペン（自由線）、点、直線（Shift+ドラッグ）、消しゴム、範囲消去
- **線幅**: 1 / 2 / 4 / 8 / 16 / 32 px
- **色**: RGB + カラーパレット
- **レイヤー**: 複数枚（下絵は編集不可）、表示切替、不透明度、並べ替え
- **Undo / Redo**: 変更矩形の差分を持つコマンド方式。100 回分保持（設定可能）
- **保存**: 独自形式 `.paint`（レイヤー情報・画素・色・線幅・キャンバス寸法）
- **書き出し**: PNG（透過対応）

## ドキュメント

- [アーキテクチャ](docs/ARCHITECTURE.md) — クレート構成、依存方向、将来機能の置き場所
- [開発ルール](docs/DEVELOPMENT.md) — GitHub Flow、Issue/PR、テスト方針
- [テスト](docs/TESTING.md) — unit / integration / E2E の考え方と実行方法
- [性能](docs/PERFORMANCE.md) — 起動時間・メモリの計測方法と実測値
- [ロードマップ](docs/ROADMAP.md) — 今後追加する機能と、その受け皿

## 開発ルール（抜粋）

- `main` への直接コミット禁止。Issue → ブランチ → Pull Request → レビュー
- すべての変更にテストを付ける
- CI（fmt / clippy / test / build / Windows E2E）が緑であること

詳しくは [docs/DEVELOPMENT.md](docs/DEVELOPMENT.md) を参照してください。

## ライセンス

MIT
