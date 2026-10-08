<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="assets/logo-dark.svg">
    <img src="assets/logo.svg" alt="miwa" width="320">
  </picture>
  <br>
  doc・req・test の追随漏れを CI で止める
</p>

<p align="center">
  <a href="#miwa-とは">miwa とは</a> &bull;
  <a href="#例えば">例えば</a> &bull;
  <a href="#使い始める">使い始める</a> &bull;
  <a href="#ドキュメント">ドキュメント</a> &bull;
  <a href="#開発に参加する">開発に参加する</a>
</p>

## miwa とは

miwa（三和）は、業務要件（doc）・技術要件（req）・テスト（test）の3層を依存関係で結び、どれかが更新されたのに残りが追随していない状態を検知する CLI です。Rust で書かれています。

doc を直したら req を、req を直したらテストを見直す。こうした約束はレビューで見落とされがちです。miwa は3層の内容を前回合意した時点と比べ、ずれた依存関係を機械的に報告します。人は「どう直すか」の判断に集中できます。

実装コードそのものは扱いません。テストを実装側の仕様とみなし、ドキュメントとテストの追随だけを見ます。機械的な検知（ハッシュ比較）は CI で使い、意味が合っているかの判定（`judge`）はローカルでだけ使います。

テストのマーカーを読み取れる言語は、Go・Rust・C#・Java・Kotlin・TypeScript・JavaScript です。

## 例えば

miwa を使うと、次のことができます。

- req を変えたのにテストが古いままの PR を、CI で失敗させる
- どの doc からも参照されていない req や、テストが1件もない req を見つける
- 変更が妥当だと判断した依存関係を、理由付きで「合意済み」として記録する
- doc と req が意味的に食い違っていないかを、AI のスコアで確かめる
- 文書の索引（`index.md`）を保ち、GitHub を使わない人もブラウザで読めるサイトを作る

doc・req・test は、それぞれ次のように書いてつなぎます。doc は Markdown のリンクで req を参照し、test はコメントのマーカーで req の id を指します。ここでは `docs/auth.md`、`req/auth/password-min-length.md`、`tests/auth_test.go` の3つのファイルに置きます。

```markdown
---
type: Specification
id: DOC-AUTH
title: "ログイン"
description: "メールアドレスとパスワードで認証する。"
---
# ログイン
ユーザーはメールアドレスとパスワードで認証する。
パスワードは [最小長の制約](../req/auth/password-min-length.md) に従う。
```

```markdown
---
type: Requirement
id: FR-001
---
8文字未満のパスワードは登録時に拒否する。
```

```go
// [FR-001]
func TestRejectsShortPassword(t *testing.T) { /* ... */ }
```

この状態で `miwa index` を実行して索引を作り、`miwa reconcile --all` で合意として記録したとします。そのあと req の「8文字未満」を「12文字未満」に変えると、`miwa check --gate` は次のように報告し、終了コード 2 を返します。

```
drift Edge doc-req:DOC-AUTH:FR-001 ReqChanged edge doc-req:DOC-AUTH:FR-001 drifted from the agreed state
drift Edge test-req:tests:TestRejectsShortPassword:FR-001 ReqChanged edge test-req:tests:TestRejectsShortPassword:FR-001 drifted from the agreed state
```

doc とテストを見直して問題がなければ、`miwa reconcile` で新しい内容を合意として記録し直します。

## 使い始める

### インストール

前提として、Rust ツールチェーン（[rustup](https://rustup.rs/)）が必要です。GitHub Releases にある利用したいタグを指定して、Git からインストールします。

```bash
cargo install --git https://github.com/NikoSanko/miwa --tag vX.Y.Z --locked
```

タグを固定すると、誰がいつ入れても同じバージョンになります。最新の main を入れる場合は、`--tag` を省略します。

private リポジトリのため、GitHub の認証が必要です。

- HTTPS: `gh auth setup-git` などで credential helper を設定します
- SSH: `--git ssh://git@github.com/NikoSanko/miwa.git` の形で指定します

リポジトリをクローンし、その中で `cargo install --path . --locked` を実行しても入れられます。

タグを push すると、GitHub Releases に macOS / Linux / Windows のバイナリが添付される予定です（[`.github/workflows/release.yml`](.github/workflows/release.yml)）。Release には `SHA256SUMS` も添付されます。ダウンロードしたアーカイブと `SHA256SUMS` を同じディレクトリに保存し、展開前に次のコマンドで検証してください。macOS では `sha256sum` の代わりに `shasum -a 256` を使います。

```bash
sha256sum --ignore-missing -c SHA256SUMS
# macOS
shasum -a 256 --ignore-missing -c SHA256SUMS
```

検証できたらアーカイブを展開し、`miwa`（Windows では `miwa.exe`）を PATH の通った場所に置きます。

今後の配布計画は [Issue #6](https://github.com/NikoSanko/miwa/issues/6) を参照してください。

### 既存プロジェクトへの導入

1. `miwa init` を実行して `.miwa/config.yaml` を作ります
2. `.miwa/config.yaml` で、doc・req・test を探すフォルダを指定します
3. doc・req・テストのマーカーを整えます
4. `miwa reconcile --all` で、現状を合意として記録します
5. `.miwa/` をコミットします
6. CI で `miwa check --gate`（または `--diff`）を実行します

詳しくは [miwa を使い始める](docs/setup.md) を参照してください。

### コマンド

| コマンド | 役割 | 詳細 |
| --- | --- | --- |
| `miwa init` | 既定値の `.miwa/config.yaml` を作る | [使い始める](docs/setup.md) |
| `miwa check` | 前回の合意と比べて、追随漏れや孤立した req などを報告する | [check](docs/check.md) |
| `miwa reconcile` | 依存関係の現在の内容を、合意としてロックに記録する | [reconcile](docs/reconcile.md) |
| `miwa judge` | `check` が報告した依存関係の両端が、意味的に整合しているかを判定する | [judge](docs/judge.md) |
| `miwa index` | 文書のタイトルと要約から、索引（`index.md`）を更新する | [index](docs/index-command.md) |
| `miwa site` | 索引の木を、ブラウザで読める静的な HTML 一式として出力する | [site](docs/site.md) |

終了コードは、成功が 0、設定不備などの実行エラーが 1 です。`check --gate` は、finding が1件でもあれば 2 を返します。

`judge` は、対象の doc・req・test の本文を TypeSafe AI（`api.typesafe.ai`）に送ります。環境変数 `TYPESAFE_API_KEY` が必要です。社外に出せない内容を含むリポジトリでは、事前に確認してください。

## ドキュメント

使い方の詳細は `docs/` にまとめています。

- [doc・req・test の3層モデルとファイルの書き方](docs/model.md)（言語ごとのテストのマーカーの書き方を含みます）
- [miwa を使い始める](docs/setup.md)（設定ファイルの全項目）
- [更新漏れを検知する `check`](docs/check.md)
- [合意を記録する `reconcile`](docs/reconcile.md)
- [意味が合っているかを判定する `judge`](docs/judge.md)
- [索引を保つ `index`](docs/index-command.md)
- [閲覧サイトを作る `site`](docs/site.md)
- [α版の要件・仕様](docs/spec-alpha.md) / [詳細検討の参考資料](docs/spec-v2-reference.md)

## 開発に参加する

リポジトリをクローンし、テストとリンターが通ることを確かめます。

```bash
git clone https://github.com/NikoSanko/miwa.git
cd miwa
cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt --check
```

このリポジトリ自身も miwa でチェックしています。`.miwa/config.yaml` では、`docs` を doc、`req` を req、`src` と `tests` を test の探索対象にしています。CI でも `miwa check --gate` を実行しているので、仕様を変えるときは `docs/` と `req/` も合わせて更新してください。

次に取り組む予定のことは、Issue にまとめています。

- [Issue #3](https://github.com/NikoSanko/miwa/issues/3): test の十分性を検証する方法（検討中）
- [Issue #4](https://github.com/NikoSanko/miwa/issues/4): Anthropic API を直接呼び出す版の `judge`（優先度低）

## ライセンス

[MIT License](LICENSE)
