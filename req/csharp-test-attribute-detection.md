---
type: Requirement
id: FR-049
---
C# では、メソッドに次のいずれかの属性が付与されている場合に、そのメソッドを test として抽出する。
- `Fact`（xUnit）
- `Theory`（xUnit / NUnit）
- `Test` `TestCase` `TestCaseSource`（NUnit）
- `TestMethod` `DataTestMethod`（MSTest）

属性名は名前空間修飾を除いた最後の識別子で比較する。そのため `[Xunit.Fact]` や `[global::Xunit.Fact]` も対象とし、引数の有無も問わない（`[Fact(Skip = "x")]` も対象）。`[method: Fact]` のような target 指定付きの属性も対象とする。`[Trait("a", "b"), Fact]` のように1つの属性リストに複数の属性が並ぶ場合も同様である。
一方、`[FactAttribute]` のように `Attribute` 接尾辞を付けた書き方と、上記以外の属性のみを持つメソッドは対象外とする。`#if` / `#endif` の中にあるメソッドも対象とする。
