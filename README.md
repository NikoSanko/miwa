# miwa（三和）

業務要件（doc）・技術要件（req）・テスト（test）の3層を依存関係で結び、どれかが更新されたのに残りが追随していない状態を機械的に検知する Rust 製 CLI ツールです。

実装コードそのものは扱わず、テストを実装側の仕様とみなして「ドキュメントとテストの追随」だけを見ます。機械的な検知（ハッシュ比較）は CI で使い、意味的に矛盾しているかどうかの判定（Judge）はローカルでだけ使う、という役割分担を前提にしています。

## 仕様

- [docs/spec-alpha.md](docs/spec-alpha.md) — α版の要件・仕様
- [docs/spec-v2-reference.md](docs/spec-v2-reference.md) — 詳細検討の参考資料
- 非開発者向けの説明:
  - [docs/model.md](docs/model.md) — doc / req / test の3層モデルとファイルの書き方
  - [docs/check.md](docs/check.md) — 更新漏れを検知する `check`
  - [docs/reconcile.md](docs/reconcile.md) — 合意を記録する `reconcile`
  - [docs/judge.md](docs/judge.md) — 意味が合っているか判定する `judge`
  - [docs/setup.md](docs/setup.md) — 使い始め方

## インストール

```bash
cargo install --path .
```

## 使い方

### `miwa init`

カレントディレクトリに `.miwa/config.yaml` を既定値で作成します。既に存在する場合は失敗します。

### `miwa check [--gate] [--diff [--base <ref>]] [--format text|json]`

doc・req・test の依存関係（エッジ）を、前回合意した内容（ロック）と比較して drift・new・removed・orphan・no-test・invalid・broken を報告します。

- `--gate`: finding が1件でもあれば exit 2 を返します（CI での利用を想定）
- `--diff`: `diff_base`（既定は設定値、`--base` で上書き可）との merge-base と比較し、新たに発生した finding だけを報告します
- `--format`: `text`（既定・人が読みやすい形式）または `json`（機械可読な形式）

終了コード: `0`（問題なし） / `1`（設定不備などの実行エラー） / `2`（`--gate` 指定時に finding あり）

### `miwa reconcile --edge <id>... | --all [--reason <text>]`

指定したエッジ、または全エッジの現在のハッシュを「合意済み」としてロック（`.miwa/lock.json`）に記録します。`--reason` で合意の理由を残せます。リンク切れ（broken）や不正な状態（invalid）のエッジは reconcile できません。

終了コード: `0`（成功） / `1`（実行エラー）

### `miwa judge [--edge <id>]... [--auto-reconcile]`

`check` が `drift` / `new` とした依存関係について、両端が意味的に整合しているかを判定して助言します。`--edge` で対象を絞れます（対象でない ID はエラー）。

1. 一次判定（Jev）: TypeSafe AI の Jev で 0〜1 のスコアを取ります。環境変数 `TYPESAFE_API_KEY` が必要です。スコアが `judge.threshold` 以上なら `consistent`、未満なら `review` です。
2. 二次判定: `review` の依存関係をまとめて、設定 `judge.llm_command` のエージェント（既定は `claude -p --allowedTools "Bash(miwa reconcile:*),Read"`）に1回だけ委任します。結論と理由はエージェントが報告し、miwa は加工せずそのまま表示します。

`--auto-reconcile` を付けると、Jev が `consistent` とした依存関係を miwa がロックに合意（`by: jev`）し、エージェントにも整合と判断したものの `reconcile` を許可します。付けない場合は、表示と報告だけでロックは書き換えません。

出力例（1依存関係につき1行、識別子順。この後にエージェントの報告が続きます）:

```
consistent test-req:internal/auth:TestRejectsShortPassword:FR-001 score=0.94
review doc-req:DOC-AUTH:FR-001 score=0.31
skipped doc-req:DOC-BIG:FR-090 score=-
```

終了コード: `0`（判定が最後まで完了。`review` / `skipped` があっても 0） / `1`（実行エラー: 設定・API キーの問題、通信エラー、エージェントの失敗など）

- `removed` は対象外です。復元するか `reconcile --edge` で削除を承認するか、人が判断してください。
- 対象の doc・req・test の本文は TypeSafe AI（`api.typesafe.ai`）と、設定したエージェント CLI（およびその先の LLM サービス）に送られます。社外に出せない内容を含むリポジトリでは事前に確認してください。
- `judge.llm_command` はリポジトリの設定ファイルから読んで実行されるため、信頼できないリポジトリ（他人の clone やブランチ）では `judge` を実行しないでください。
- `judge` はローカル専用の助言コマンドです。CI では `check --gate` を使います。

## 設定

`.miwa/config.yaml` のうち `judge` に関する項目です（他の項目は [docs/setup.md](docs/setup.md) を参照）。

- `judge.threshold`: Jev のスコアがこの値以上なら `consistent` とみなします。0〜1 の範囲で、既定値は `0.9`
- `judge.llm_command`: 二次判定を任せるエージェント CLI を、コマンドと引数の配列で指定します。既定値は `["claude", "-p", "--allowedTools", "Bash(miwa reconcile:*),Read"]`（空にはできません）

## ファイルの書き方

### doc（業務要件）

```markdown
---
type: Specification
id: DOC-AUTH
---
# ログイン
ユーザーはメールアドレスとパスワードで認証する。
パスワードは [最小長の制約](../req/auth/password-min-length.md) に従う。
```

### req（技術要件）

```markdown
---
type: Requirement
id: FR-001
---
8文字未満のパスワードは登録時に拒否する。
```

### test（Go）

```go
// [FR-001]
func TestRejectsShortPassword(t *testing.T) { ... }
```

### test（Rust）

```rust
// [FR-001]
#[test]
fn rejects_short_password() { ... }
```

詳しい書き方・制約は [docs/model.md](docs/model.md) を参照してください。

## このリポジトリ自身が miwa でチェックされている

miwa 自身を miwa でチェックする（dogfooding）ために、Rust を対応言語に含めています。`.miwa/config.yaml` で、`docs` は `docs`、`requirements` は `req`、`tests` は `src` と `tests` を探索対象に設定しています。

## 開発

```bash
cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt
```

## 次のスコープ

- [Issue #3](https://github.com/NikoSanko/miwa/issues/3) — test の十分性の検証方法（検討中）
- [Issue #4](https://github.com/NikoSanko/miwa/issues/4) — Anthropic API を直接呼び出す版の `judge`（優先度低）
