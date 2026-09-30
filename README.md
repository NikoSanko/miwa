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

`judge`（Jev / LLM による意味判定）は Phase B で実装予定です。
