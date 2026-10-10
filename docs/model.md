---
type: BusinessNeed
id: DOC-MODEL
title: "miwa の3層モデル"
description: "doc・req・test の3層の関係と、それぞれの書き方、変更の検知に使う範囲、エッジの識別子を説明する。"
---

# miwa の3層モデル

miwa は、「業務要求（doc）」「システム要件（req）」「テスト（test）」という3つの文書・コードを結び付け、どれかが更新されたのに残りが追随していない状態を見つけるツールです。

ここでの「要求」は業務として実現したいことを、「要件」はそれを検証できる形に具体化したもの（機能・非機能を含む）を指します。

```
doc（業務要求） ── 依存 ──▶ req（システム要件） ◀── 検証 ── test（テストコード）
```

- 1つの doc は複数の req に依存できます。1つの req は複数の doc から依存されてかまいません。
- 1つの req は複数の test で検証できます。ただし1つの test が検証する req は必ず1つです。

## doc（業務要求）の書き方

doc は、非開発メンバーが読む、業務要求を書いた文書です。ファイルの先頭に次のような情報（フロントマター）を書き、`type: BusinessNeed` を付けます。

```markdown
---
type: BusinessNeed
id: DOC-AUTH
---
# ログイン
ユーザーはメールアドレスとパスワードで認証する。
```

`type` の書かれていない Markdown ファイルはノードとして扱われず、miwa の対象になりません（[type を持たないファイルの扱い](../req/node-requires-type-field.md)）。

`index.md` と `log.md` は予約名です。`type` を書いていても、どのフォルダでも doc や req として扱いません（[予約名の規則](../req/index-reserved-names.md)）。索引については [索引を保つ](index-command.md) を参照してください。

doc には、`id` のほかに `title`（任意）と `description`（1文の要約）を書けます。`title` は索引のタイトル、`description` は索引の要約の元になります。`title` が無ければ本文の最初のレベル 1 見出しが使われ、それも無ければファイル名が使われます。`description` は1行の文字列です（[title と description の規則](../req/index-document-title-and-description.md)）。

```markdown
---
type: BusinessNeed
id: DOC-AUTH
title: "ログイン"
description: "メールアドレスとパスワードによる認証の振る舞いを定める。"
---
```

## req（システム要件）の書き方

req は、検証可能な1つの振る舞いを「〜する」「〜を返す」の形で書いた短い文書です。doc と同じくフロントマターに `type: Requirement` を付けます。

```markdown
---
type: Requirement
id: FR-001
---
8文字未満のパスワードは登録時に拒否する。
```

`id` に使える文字は、半角英数字と `.` `_` `-` だけです（[id に使える文字の規則](../req/req-id-allowed-characters.md)）。これ以外の文字を使うと、不正な文書として報告されます。`id` が空・未設定の場合や、同じ `id` を持つファイルが複数ある場合も、不正として報告されます（[id の欠落・重複の規則](../req/id-missing-or-duplicate-is-invalid.md)）。

## doc から req へのリンク

doc の本文中で req ファイルへ相対リンクを張ると、それが doc → req の依存関係になります。

```markdown
パスワードは [最小長の制約](../req/auth/password-min-length.md) に従う。
```

通常のリンクと、脚注のような参照形式のリンクは、どちらも依存関係として認識されます（[リンク形式の規則](../req/doc-req-link-relative-resolution.md)）。リンクに `#見出し` のようなフラグメントが付いていても、フラグメントを取り除いてリンク先を特定します（[フラグメントの扱い](../req/doc-req-link-strips-fragment.md)）。

一方で、外部サイトへの URL、画像、req 置き場の外を指すリンクは、依存関係として扱いません（[対象外のリンク](../req/doc-req-link-excluded-targets.md)）。req 置き場の中を指しているのにリンク先が見つからない場合や、見つかったファイルが req でない場合は、「壊れたリンク」として報告されます（[壊れたリンクの規則](../req/doc-req-link-broken-target.md)）。

## test（テストコード）の書き方

test は、req が実際に検証されていることを示すコードです。miwa は現時点で Go・Rust・C#・Java・Kotlin（JUnit）・TypeScript / JavaScript（Vitest）の6言語に対応しています。

どの言語でも、マーカーに採用するのは行頭から始まるコメントだけです（[マーカーコメントの規則](../req/marker-comment-own-line.md)）。前の宣言の末尾に書いたコメント（`fn x() {} // [FR-001]` など）は、次の test の直前にあっても拾いません。同じ行に複数のコメントがある行は、その行のコメントを読まず、マーカーの探索をそこで打ち切ります。

### Go

```go
// [FR-001]
func TestRejectsShortPassword(t *testing.T) { ... }
```

`*_test.go` に書かれたトップレベルの `func TestXxx` の直前に、空行を挟まずにコメントで `[req id]` を書くと、それが test → req の依存関係になります。空行を挟んだ場合や、コメント自体がない場合は、マーカーなしとして無視されます（[マーカーの隣接条件](../req/go-test-marker-adjacency.md)）。

Go の test の識別子は、テストファイルが置かれたディレクトリと関数名を組み合わせた形式です（[識別子の形式](../req/go-test-identifier-format.md)）。

### Rust

```rust
// [FR-001]
#[test]
fn rejects_short_password() { ... }
```

Rust では、属性パスの末尾が `test` の属性が付いた関数が対象です。`#[test]`、`#[tokio::test]`、`#[async_std::test]` などが該当し、引数付きの属性も対象です（[属性の検出規則](../req/rust-test-attribute-detection.md)）。他の属性の引数の中に現れる `test`（`#[cfg_attr(x, test)]` など）は見ません。

`#[rstest]` のようにこれに当たらない属性は、`.miwa/config.yaml` の `rust.test_attributes` に名前を列挙すると対象になります。既定は空です（[設定の規則](../req/config-rust-test-attributes.md)）。

マーカーは、その関数に付いた test 属性のうち、ソース上で最初に現れるものの直前に、空行を挟まず隣接するコメントに書きます（[マーカーの隣接条件](../req/rust-test-marker-adjacency.md)）。属性同士の間や、属性と関数の間のコメントはマーカーになりません。

doc コメント（`///` `//!` `/** */` `/*! */`）は rustdoc の `[Foo]` リンクと衝突するため、マーカーになりません。ただし、マーカーコメントと test 属性の間に doc コメントが挟まっていても、マーカーは有効です。

Rust の test の識別子は、ファイルパスとモジュールの階層、関数名を組み合わせた形式です（[識別子の形式](../req/rust-test-identifier-format.md)）。

### C#

```csharp
public class AuthTests
{
    // [FR-001]
    [Fact]
    public void RejectsShortPassword() { ... }
}
```

C# では、`Fact` `Theory` `Test` `TestCase` `TestCaseSource` `TestMethod` `DataTestMethod` のいずれかの属性が付いたメソッドが対象です（[属性の検出規則](../req/csharp-test-attribute-detection.md)）。xUnit・NUnit・MSTest に対応します。`[Xunit.Fact]` のような名前空間付きや `[Fact(Skip = "x")]` のような引数付きも対象ですが、`[FactAttribute]` の書き方は対象外です。

マーカーは、メソッド宣言（属性を含む）の直前に空行を挟まないコメントに書きます（[マーカーの隣接条件](../req/csharp-test-marker-adjacency.md)）。属性と署名の間に書いたコメントはマーカーになりません。

C# の test の識別子は、ファイルパスと型名の階層、メソッド名を組み合わせた形式です（[識別子の形式](../req/csharp-test-identifier-format.md)）。namespace は含みません。例えば `tests/AuthTests.cs` の `class AuthTests` 内の `RejectsShort` は `tests/AuthTests:AuthTests:RejectsShort` になります。

### Java / Kotlin（JUnit）

Java の例です。

```java
public class AuthTest {
    // [FR-001]
    @Test
    void rejectsShortPassword() { ... }
}
```

Kotlin の例です。

```kotlin
class AuthTest {
    // [FR-001]
    @Test
    fun `短い パスワード は 拒否`() { ... }

    @Nested
    inner class 一覧取得 {
        // [FR-002]
        @Test
        fun 取得() { ... }
    }
}
```

Java と Kotlin で対象になるファイルは、`tests` 配下の `*.java` と `*.kt` です。`*.kts` は対象外です。

`Test` `ParameterizedTest` `RepeatedTest` `TestFactory` `TestTemplate` のいずれかの注釈が付いたメソッド（Kotlin では関数）が対象です（[注釈の検出規則](../req/java-kotlin-test-annotation-detection.md)）。`@org.junit.jupiter.api.Test` のような修飾付きや `@Test(timeout = 1)` のような引数付きも対象です。抽出するのは型の直下のメソッドだけで、メソッドの本体の中、匿名オブジェクト、enum 定数の本体、プロパティの中は対象外です。Kotlin ではトップレベル関数も対象です。

マーカーは、メソッド宣言（注釈・修飾子を含む）の直前に空行を挟まない `//` または `/* */` のコメントに書きます（[マーカーの隣接条件](../req/java-kotlin-test-marker-adjacency.md)）。Javadoc / KDoc（`/** */`）や、注釈と署名の間に書いたコメントはマーカーになりません。

Java / Kotlin の test の識別子は、ファイルパスと型名の階層、メソッド名を組み合わせた形式です（[識別子の形式](../req/java-kotlin-test-identifier-format.md)）。package は含みません。上の Kotlin の例は `src/test/kotlin/AuthTest:AuthTest:短い パスワード は 拒否`（バッククォートは除かれます）と `src/test/kotlin/AuthTest:AuthTest/一覧取得:取得` になります。

Kotlin では、漢字・カタカナ・ハングルなどの非 ASCII 文字を識別子に含むテストも抽出できます（[非 ASCII 識別子の規則](../req/kotlin-non-ascii-identifiers.md)）。

### TypeScript / JavaScript（Vitest）

```typescript
describe("auth", () => {
  // [FR-001]
  it("rejects short password", () => { ... });
});
```

TypeScript / JavaScript で対象になるファイルは、`<name>.test.<ext>` と `<name>.spec.<ext>` です。ext は `ts` `tsx` `mts` `cts` `js` `jsx` `mjs` `cjs` のいずれかです（[ファイル名の規則](../req/typescript-test-file-patterns.md)）。`.test` / `.spec` を含まないファイルは対象外です。`.ts` `.mts` `.cts` は TypeScript 構文、それ以外は TSX 構文（JSX を含められる）として解析します。

同名の `.test.js` と `.test.ts` が共存すると、識別子が同じになり、1つの test として扱われます。そのため、tsc の出力などの生成物のディレクトリは、[`exclude` で除外する](../req/config-exclude-dirs.md)か、`tests` に含めないでください。

ファイル直下、または `describe` のコールバック直下にある `test(...)` / `it(...)` の呼び出し文が、test として抽出されます（[呼び出し文の検出規則](../req/typescript-test-call-detection.md)）。`it.skip` `test.only` などの修飾子付きも対象です。`test.each` や、関数の中・test の中にある `it` は無視されます。

マーカーは、呼び出し文の直前に空行を挟まないコメントに書きます（[マーカーの隣接条件](../req/typescript-test-marker-adjacency.md)）。

TypeScript / JavaScript の test の識別子は、ファイルパスと describe 名の階層、test 名を組み合わせた形式です（[識別子の形式](../req/typescript-test-identifier-format.md)）。上の例は `src/auth.test:auth:rejects short password` になります。

## 変更の検知に使う範囲

miwa は doc・req・test それぞれの「内容」をハッシュ値にして、前回合意した内容と比べます。ハッシュの計算には SHA-256 を使い、改行コードの違いだけは無視します（[ハッシュの規則](../req/hash-sha256-with-lf-normalization.md)）。

- doc・req は、フロントマターを除いた本文だけが比較対象です（[比較範囲の規則](../req/hash-doc-req-scope-excludes-frontmatter.md)）。`id` だけを書き換えても、内容の変更とはみなされません。
- 索引の対象になっている doc・req には、要約（`description`）と本文の対を見る [`desc` エッジ](../req/desc-edge-exists-for-indexed-nodes.md)があります。要約と本文のハッシュは別々に取ります（[ハッシュの取り方](../req/desc-edge-hash-sources.md)）。そのため、`description` だけを変えても doc → req や test → req のエッジには影響しません。本文の編集は、doc → req のエッジと `desc` エッジの両方に現れます。
- test は、関数の定義全体が比較対象です（[比較範囲の規則](../req/hash-test-scope-is-whole-function.md)）。言語ごとの扱いは次のとおりです。
  - Rust は、`#[test]` などの属性とマーカーコメントをハッシュの対象に含めません（[Rust の除外範囲](../req/hash-rust-test-excludes-outer-attributes.md)）。
  - C# は、メソッドに付けた属性（`[Fact]` など）と属性直後のコメントをハッシュの対象に含めません（[C# の除外範囲](../req/hash-csharp-test-excludes-attributes.md)）。
  - Java / Kotlin は、先頭に連続する注釈（`@Test` など）と、その間・直後のコメントをハッシュの対象に含めません（[Java / Kotlin の除外範囲](../req/hash-java-kotlin-test-excludes-annotations.md)）。修飾子より後ろに書いた注釈は含みます。
  - TypeScript / JavaScript は、`test(...)` / `it(...)` の呼び出し文全体が対象です。

## エッジの識別子

doc → req、test → req、そして要約と本文の対のそれぞれの依存関係（エッジ）には、識別子が付きます。doc → req は `doc-req:<doc の id>:<req の id>`、test → req は `test-req:<test の識別子>:<req の id>` という形式です（[エッジ識別子の形式](../req/edge-id-format.md)）。

索引の対象になっているノードの要約と本文の対は、Doc なら `desc:doc:<doc の id>`、Req なら `desc:req:<req の id>` です（[desc エッジの規則](../req/desc-edge-exists-for-indexed-nodes.md)）。`desc` エッジの識別子は、Doc と Req が同じ id を持っていても衝突しません。

この識別子は、`check` の結果やロックファイルの中で、依存関係を指し示すために使われます。

マーカーを持つ test の識別子がリポジトリ内で重複している場合（C# や Java のオーバーロードなど）は、それらの出現が1つの test にまとめられ、内容はソース順に連結されます（[重複の扱い](../req/test-duplicate-identifier-merged.md)）。まとめられた出現のマーカーが異なる req を指している場合だけ、不正として報告されます。
