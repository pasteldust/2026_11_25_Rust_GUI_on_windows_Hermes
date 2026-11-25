# 開発ルール

## ブランチ運用（GitHub Flow）

- `main` は常にビルドでき、テストが通る状態に保つ。**`main` へ直接コミットしない。**
- 作業は必ず Issue から始める。ブランチ名は `<種類>/<Issue番号>-<短い説明>`:

  | 種類 | 用途 | 例 |
  | --- | --- | --- |
  | `feat/` | 機能追加 | `feat/12-line-tool` |
  | `fix/` | 不具合修正 | `fix/31-eraser-alpha` |
  | `docs/` | ドキュメント | `docs/40-readme` |
  | `chore/` | 雑務・CI | `chore/2-workspace` |
  | `test/` | テスト | `test/50-history` |

- **例外（初回の 1 回だけ）**: 空のリポジトリでは PR の土台になる `main` が存在しないため、
  最初の 1 コミット（workspace の骨組み）だけは `main` に直接入れる。以後は必ず PR 経由。

- 変更は Pull Request 経由で `main` に入れる。PR の本文には次を書く:
  1. 対応する Issue（`Closes #12`）
  2. 変更点の要約
  3. **レビュー観点**（どこを危険と見ているか、何を確認してほしいか）
  4. テストの内容と実行結果
  5. 未対応・既知の制限

## コミットメッセージ

```
<種類>(<範囲>): <要約>

本文（なぜ変えたか。何を変えたかは差分を読めば分かる）
```

種類は `feat` / `fix` / `docs` / `test` / `refactor` / `chore` / `perf` を使う。

## すべての変更にテストを付ける

- 振る舞いを変えたら、その振る舞いを固定するテストを同じ PR に含める。
- GUI に依存しない部分（`paint-core` / `paint-engine` / `paint-persist` / ViewModel）は
  **必ず自動テストで検証できる形にする**。GUI の描画そのものは手動確認と E2E の自己テストで補う。
- テスト用の画像ファイルはリポジトリに置かず、テストの中で生成する（バイナリを持たない）。

## CI

`.github/workflows/ci.yml` が PR ごとに走る。

| ジョブ | 内容 |
| --- | --- |
| `cargo fmt` | 整形の確認（`--check`） |
| `cargo clippy` | `-D warnings` で警告をエラー扱い |
| `cargo test` | Ubuntu と Windows の両方で全テスト |
| `cargo build --release` | 両 OS で release ビルド。exe を成果物として保存 |
| `E2E (windows-latest)` | `paint-app --self-test` で起動・描画・保存・再読込・Undo・Redo を確認 |

CI が赤いままの PR はマージしない。

Linux のジョブは eframe のビルドに必要な X11 / GL / Wayland のヘッダを `apt-get` で入れてから走らせる
（入っていないと `cargo clippy` や `cargo build` がリンクの段階で落ちる）。

## 実装で迷ったときの判断

1. **推測で実装しない。** 仕様が決まっていない点は Issue を立てて Product Owner の判断を仰ぐ。
2. **層をまたぐ依存を増やさない。** `paint-core` に GUI やファイル形式の都合を持ち込まない。
3. **公開 API にはドキュメントコメントを書く。** `#![warn(missing_docs)]` を有効にしてある。
4. **`unsafe` を書かない。** 全クレートで `#![forbid(unsafe_code)]`。
