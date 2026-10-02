---
type: Requirement
id: FR-059
---
Java と Kotlin では、`tests` に設定されたディレクトリ配下の `*.java` と `*.kt` を抽出対象ファイルとし、`*.kts` は対象外とする。メソッド（Kotlin では関数）に `Test` `ParameterizedTest` `RepeatedTest` `TestFactory` `TestTemplate` のいずれかの注釈が付与されている場合に、それを test として抽出する。注釈名は修飾を除いた最後の識別子で比較するため、`@org.junit.jupiter.api.Test` も対象とし、引数の有無も問わない（`@Test(timeout = 1)` も対象）。Kotlin の `@[Test Tag("a")]` のように 1 つの注釈に複数を並べる形式や、`@get:Test` のような use-site target 付きの注釈も、target を無視して対象とする。上記以外の注釈のみを持つメソッドは対象外とする。

抽出するのは型の直下に宣言されたメソッドだけとする。走査する型は、Java では class / record / interface / enum、Kotlin では class（inner / data / enum / interface を含む）/ object / companion object とし、入れ子の型も辿る。Java のトップレベルのメソッドは対象外、Kotlin のトップレベル関数は対象とする。メソッド（関数）の本体の中、匿名オブジェクト、enum 定数の本体、プロパティの中にある宣言は対象外とする。
