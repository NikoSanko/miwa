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
  - [docs/index-command.md](docs/index-command.md) — 索引を保つ `index`
  - [docs/site.md](docs/site.md) — 閲覧サイトを作る `site`
  - [docs/setup.md](docs/setup.md) — 使い始め方

## インストール

前提: Rust ツールチェーン（[rustup](https://rustup.rs/)）。

### 推奨: タグを指定して Git からインストール

```bash
cargo install --git https://github.com/NikoSanko/miwa --tag vX.Y.Z --locked
```

`vX.Y.Z` は GitHub Releases にある利用したいタグに置き換えます。タグを固定することで、誰がいつ入れても同じバージョンになり再現性を確保できます。最新の main を入れる場合は `--tag` を省略します。

```bash
cargo install --git https://github.com/NikoSanko/miwa --locked
```

private リポジトリのため GitHub の認証が必要です。

- HTTPS: `gh auth setup-git` などで credential helper を設定します
- SSH: `--git ssh://git@github.com/NikoSanko/miwa.git` の形で指定します

### 開発者向け

リポジトリをクローンしてインストールします。

```bash
cargo install --path . --locked
```

### ビルド済みバイナリ

タグを push すると、GitHub Releases に macOS / Linux / Windows のバイナリが添付される予定です（[`.github/workflows/release.yml`](.github/workflows/release.yml)）。ダウンロードして PATH の通った場所に置きます。

Release には `SHA256SUMS` が添付されます。ダウンロードしたアーカイブと `SHA256SUMS` を同じディレクトリに保存し、展開前に次のコマンドで検証してください。

```bash
sha256sum --ignore-missing -c SHA256SUMS
```

今後の配布計画は [Issue #6](https://github.com/NikoSanko/miwa/issues/6) を参照してください。

## 既存プロジェクトへの導入

1. `miwa init` を実行します
2. `.miwa/config.yaml` を編集します
3. doc / req / マーカーを整備します
4. `miwa reconcile --all` で現状の合意を記録します
5. `.miwa/` をコミットします
6. CI で `miwa check --gate`（または `--diff`）を実行します

詳細は [docs/setup.md](docs/setup.md) を参照してください。

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

### `miwa index`

`.miwa/config.yaml` の `index` に設定した根ディレクトリの索引（`index.md`）を、文書のタイトル（`title`）と要約（`description`）から更新します。索引が無ければ新規に作り、未掲載の文書は末尾に追加します。リンク先が存在しないエントリ行は削除します。エントリ行以外は変更しません。`check` が `index` 種別の finding として索引の乱れを報告します。

終了コード: `0`（成功） / `1`（実行エラー。`index` 未設定を含む）

### `miwa judge [--edge <id>]... [--auto-reconcile]`

`check` が `drift` / `new` とした依存関係について、両端が意味的に整合しているかを判定して助言します。`--edge` で対象を絞れます（対象でない ID はエラー）。

Jev で 0〜1 のスコアを取ります（環境変数 `TYPESAFE_API_KEY` が必要）。スコアが `judge.threshold` 以上なら `consistent`、未満なら `review` として報告します。`review` の判断は呼び出し元（人や LLM）に任せ、miwa は理由の文章を作りません。

`--auto-reconcile` を付けると、Jev が `consistent` とした依存関係を miwa がロックに合意（`by: jev`）として記録します。付けない場合は、表示だけでロックは書き換えません。

出力例（1依存関係につき1行、識別子順）:

```
consistent test-req:internal/auth:TestRejectsShortPassword:FR-001 score=0.94
review doc-req:DOC-AUTH:FR-001 score=0.31
skipped doc-req:DOC-BIG:FR-090 score=-
```

終了コード: `0`（判定が最後まで完了。`review` / `skipped` があっても 0） / `1`（実行エラー: 設定・API キーの問題、通信エラーなど）

- `removed` は対象外です。復元するか `reconcile --edge` で削除を承認するか、人が判断してください。
- 対象の doc・req・test の本文は TypeSafe AI（`api.typesafe.ai`）にのみ送られます。社外に出せない内容を含むリポジトリでは事前に確認してください。
- `judge` はローカル専用の助言コマンドです。CI では `check --gate` を使います。

## 設定

`.miwa/config.yaml` のうち `judge` に関する項目です（他の項目は [docs/setup.md](docs/setup.md) を参照）。

- `judge.threshold`: Jev のスコアがこの値以上なら `consistent` とみなします。0〜1 の範囲で、既定値は `0.9`
- `judge.max_body_bytes`: 1つの依存関係で Jev に送る両端の本文の合計バイト数の上限。超えると `skipped` になります。1以上の整数で、既定値は `200000`

Rust で `rstest` など `test` 以外の名前の属性を test として扱う場合は、属性名（末尾セグメントのみ）を `rust.test_attributes` に列挙します。省略時は空です。

```yaml
rust:
  test_attributes: []   # 例: [rstest]
```

ビルド出力など探索から外したいディレクトリは、名前（パスではない）を `exclude` に列挙します。`dist` などは既定で除外されます。

```yaml
exclude: []   # 例: [build, .next]
```

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

`#[test]` のほか、`#[tokio::test]` のように属性パスの末尾が `test` の属性も対象です。`rstest` など他の名前の属性は、`.miwa/config.yaml` の `rust.test_attributes` に追加すると対象になります。マーカーは最初の test 属性の直前に書きます。

### test（C#）

```csharp
public class AuthTests
{
    // [FR-001]
    [Fact]
    public void RejectsShortPassword() { ... }
}
```

### test（Java / Kotlin / JUnit）

```kotlin
class AuthTest {
    // [FR-001]
    @Test
    fun `短い パスワード は 拒否`() { ... }
}
```

`Test` `ParameterizedTest` などの JUnit の注釈が付いたメソッドが対象です（Java も同様）。Gradle のプロジェクトでは、`.miwa/config.yaml` の `tests` に `tests: ["src/test"]` を指定します（`src/generated` は含めないでください。`build/` などのビルド出力は `exclude: ["build"]` で除外します）。

### test（TypeScript / JavaScript / Vitest）

```typescript
describe("auth", () => {
  // [FR-001]
  it("rejects short password", () => { ... });
});
```

Vitest のテストをソースと同じディレクトリに置く構成では、`.miwa/config.yaml` の `tests` にそのディレクトリを指定します（例: `tests: [src]`）。

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
