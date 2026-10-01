---
type: Requirement
id: FR-054
---
TypeScript / JavaScript（Vitest）では、ファイル直下、または有効な `describe` のコールバック本体の直下にある文のうち、`test(...)` / `it(...)` の呼び出し文を test として抽出する。`test` / `it` には `skip` `only` `todo` `fails` `concurrent` `sequential` の修飾子のみを連ねた形（`it.skip(...)` `test.concurrent.only(...)` など）も許可し、第1引数が文字列リテラルまたはテンプレート文字列であるものだけを対象とする。`describe` は `skip` `only` `todo` `concurrent` `sequential` `shuffle` の修飾子を許可し、第1引数が同様の文字列、第2引数が関数（アロー関数または function 式）のとき、その本体の直下へ降りる。次の書き方は無視する（invalid にはしない）。`test.each(...)(...)` `test.skipIf(c)(...)` `test.extend(...)` `describe.each(...)` `describe.skipIf(c)(...)` `it?.skip(...)` のように許可外の修飾子や呼び出し結果の呼び出しを含むもの、`await it(...)` `void it(...)` `export default describe(...)` のように式文そのものが呼び出しでないもの、第1引数が変数や式であるもの、および通常の関数宣言・test のコールバック本体・if や loop の内部にある `it`。
