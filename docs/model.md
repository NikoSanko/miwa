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

test は、req が実際に検証されていることを示すコードです。miwa は現時点で Go・Rust・C#・TypeScript（Vitest）に対応しています。

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

Rust では、[属性パスの末尾が `test` の属性（`#[test]`、`#[tokio::test]`、`#[async_std::test]` など。引数付きも可）が付いた関数が対象](../req/rust-test-attribute-detection.md)です。他の属性の引数の中に現れる `test`（`#[cfg_attr(x, test)]` など）は見ません。`#[rstest]` のようにこれに当たらない属性は、`.miwa/config.yaml` の `rust.test_attributes` に名前を列挙すると対象になります（[設定の規則](../req/config-rust-test-attributes.md)。既定は空）。マーカーは、その関数に付いた test 属性のうち[ソース上で最初に現れるものの直前に、空行を挟まず隣接するコメント](../req/rust-test-marker-adjacency.md)に書きます。属性同士の間や、属性と関数の間のコメントはマーカーになりません。Rust の test の識別子は、[ファイルパスとモジュールの階層、関数名を組み合わせた形式](../req/rust-test-identifier-format.md)になります。

### C#

```csharp
public class AuthTests
{
    // [FR-001]
    [Fact]
    public void RejectsShortPassword() { ... }
}
```

C# では、[`Fact` `Theory` `Test` `TestCase` `TestCaseSource` `TestMethod` `DataTestMethod` のいずれかの属性が付いたメソッドが対象](../req/csharp-test-attribute-detection.md)です（xUnit・NUnit・MSTest に対応。`[Xunit.Fact]` のような名前空間付きや `[Fact(Skip = "x")]` のような引数付きも対象で、`[FactAttribute]` の書き方は対象外です）。マーカーは、[メソッド宣言（属性を含む）の直前に空行を挟まないコメント](../req/csharp-test-marker-adjacency.md)に書きます。属性と署名の間に書いたコメントはマーカーになりません。C# の test の識別子は、[ファイルパスと型名の階層、メソッド名を組み合わせた形式](../req/csharp-test-identifier-format.md)です（namespace は含みません）。例えば `tests/AuthTests.cs` の `class AuthTests` 内の `RejectsShort` は `tests/AuthTests:AuthTests:RejectsShort` になります。

### TypeScript（Vitest）

```typescript
describe("auth", () => {
  // [FR-001]
  it("rejects short password", () => { ... });
});
```

TypeScript では、[`*.test.ts` `*.spec.ts` `*.test.tsx` `*.spec.tsx` が対象ファイル](../req/typescript-test-file-patterns.md)です。ファイル直下、または `describe` のコールバック直下にある [`test(...)` / `it(...)` の呼び出し文が test として抽出され](../req/typescript-test-call-detection.md)ます（`it.skip` `test.only` などの修飾子付きも対象。`test.each` や関数の中・test の中にある `it` は無視されます）。マーカーは、[呼び出し文の直前に空行を挟まないコメント](../req/typescript-test-marker-adjacency.md)に書きます。TypeScript の test の識別子は、[ファイルパスと describe 名の階層、test 名を組み合わせた形式](../req/typescript-test-identifier-format.md)です。上の例は `src/auth.test:auth:rejects short password` になります。

## 変更の検知に使う範囲

miwa は doc・req・test それぞれの「内容」をハッシュ値にして、前回合意した内容と比べます。ハッシュの計算には [SHA-256 を使い、改行コードの違いだけは無視します](../req/hash-sha256-with-lf-normalization.md)。

- doc・req は、[フロントマターを除いた本文だけ](../req/hash-doc-req-scope-excludes-frontmatter.md)が比較対象です。`id` だけを書き換えても内容の変更とはみなされません。
- test は、[関数の定義全体](../req/hash-test-scope-is-whole-function.md)が比較対象です。Rust の場合、[`#[test]` などの属性やマーカーコメントはハッシュの対象に含めません](../req/hash-rust-test-excludes-outer-attributes.md)。C# の場合も、[メソッドに付けた属性（`[Fact]` など）や属性直後のコメントはハッシュの対象に含めません](../req/hash-csharp-test-excludes-attributes.md)。TypeScript の場合は `test(...)` / `it(...)` の呼び出し文全体が対象です。

## エッジの識別子

doc → req、test → req のそれぞれの依存関係（エッジ）には識別子が付きます。[doc → req は `doc-req:<doc の id>:<req の id>`、test → req は `test-req:<test の識別子>:<req の id>`](../req/edge-id-format.md) という形式です。この識別子は `check` の結果やロックファイルの中で、依存関係を指し示すために使われます。

マーカーを持つ test の識別子がリポジトリ内で重複している場合（C# のオーバーロードなど）は、[それらの出現が 1 つの test にまとめられ](../req/test-duplicate-identifier-merged.md)、内容はソース順に連結されます。まとめられた出現のマーカーが異なる req を指している場合だけ、不正として報告されます。
