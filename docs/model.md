---
type: Specification
id: DOC-MODEL
---

# miwa の3層モデル

miwa は、「業務要件（doc）」「技術要件（req）」「テスト（test）」という3つの文書・コードを結び付け、どれかが更新されたのに残りが追随していないことを見つけるツールです。

```
doc（業務要件） ── 依存 ──▶ req（技術要件） ◀── 検証 ── test（テストコード）
```

- 1つの doc は複数の req に依存できます。1つの req は複数の doc から依存されてかまいません。
- 1つの req は複数の test で検証できます。ただし1つの test が検証する req は必ず1つです。

## doc（業務要件）の書き方

doc は非開発メンバーが読む業務要件の文書です。ファイルの先頭に次のような情報（フロントマター）を書き、`type: Specification` を付けます。

```markdown
---
type: Specification
id: DOC-AUTH
---
# ログイン
ユーザーはメールアドレスとパスワードで認証する。
```

`type` の書かれていない Markdown ファイルは miwa の対象になりません（[type を持たないファイルはノードとして扱わない](../req/node-requires-type-field.md)）。

## req（技術要件）の書き方

req は「〜する」「〜を返す」という、検証可能な1つの振る舞いを書いた短い文書です。doc と同じくフロントマターに `type: Requirement` を付けます。

```markdown
---
type: Requirement
id: FR-001
---
8文字未満のパスワードは登録時に拒否する。
```

`id` に使える文字には制限があります。[id に使える文字は半角英数字と `.` `_` `-` のみ](../req/req-id-allowed-characters.md)で、これ以外の文字を使うと不正な文書として報告されます。また、[id が空・未設定の場合や、同じ id を持つファイルが複数ある場合も不正として報告](../req/id-missing-or-duplicate-is-invalid.md)されます。

## doc から req へのリンク

doc の本文中で req ファイルへ相対リンクを張ると、それが doc → req の依存関係になります。

```markdown
パスワードは [最小長の制約](../req/auth/password-min-length.md) に従う。
```

[通常のリンクと、脚注のような参照形式のリンクのどちらでも](../req/doc-req-link-relative-resolution.md)依存関係として認識されます。リンクに `#見出し` のようなフラグメントが付いていても、[フラグメントを取り除いたうえでリンク先を特定](../req/doc-req-link-strips-fragment.md)します。

一方で、[外部サイトへの URL・画像・req 置き場の外を指すリンクは依存関係として扱いません](../req/doc-req-link-excluded-targets.md)。req 置き場の中を指しているのにリンク先が見つからない場合や、見つかったファイルが req でない場合は、[「壊れたリンク」として報告](../req/doc-req-link-broken-target.md)されます。

## test（テストコード）の書き方

test は、req が実際に検証されていることを示すコードです。miwa は現時点で Go と Rust に対応しています。

### Go

```go
// [FR-001]
func TestRejectsShortPassword(t *testing.T) { ... }
```

`*_test.go` に書かれたトップレベルの `func TestXxx` の直前に、空行を挟まずにコメントで `[req id]` を書くと、それが test → req の依存関係になります。空行を挟んだり、コメント自体がなかったりする場合は、[マーカーなしとして無視](../req/go-test-marker-adjacency.md)されます。Go の test の識別子は、[テストファイルが置かれたディレクトリと関数名を組み合わせた形式](../req/go-test-identifier-format.md)になります。

### Rust

```rust
// [FR-001]
#[test]
fn rejects_short_password() { ... }
```

Rust では、[`#[test]` 属性が付いた関数だけが対象](../req/rust-test-literal-test-attribute-only.md)です（`#[tokio::test]` のような属性は対象外）。マーカーの書き方は Go と同様、[`#[test]` の直前に空行を挟まないコメント](../req/rust-test-marker-adjacency.md)です。Rust の test の識別子は、[ファイルパスとモジュールの階層、関数名を組み合わせた形式](../req/rust-test-identifier-format.md)になります。

## 変更の検知に使う範囲

miwa は doc・req・test それぞれの「内容」をハッシュ値にして、前回合意した内容と比べます。ハッシュの計算には [SHA-256 を使い、改行コードの違いだけは無視します](../req/hash-sha256-with-lf-normalization.md)。

- doc・req は、[フロントマターを除いた本文だけ](../req/hash-doc-req-scope-excludes-frontmatter.md)が比較対象です。`id` だけを書き換えても内容の変更とはみなされません。
- test は、[関数の定義全体](../req/hash-test-scope-is-whole-function.md)が比較対象です。Rust の場合、[`#[test]` などの属性やマーカーコメントはハッシュの対象に含めません](../req/hash-rust-test-excludes-outer-attributes.md)。

## エッジの識別子

doc → req、test → req のそれぞれの依存関係（エッジ）には識別子が付きます。[doc → req は `doc-req:<doc の id>:<req の id>`、test → req は `test-req:<test の識別子>:<req の id>`](../req/edge-id-format.md) という形式です。この識別子は `check` の結果やロックファイルの中で、依存関係を指し示すために使われます。
