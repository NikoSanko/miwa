---
type: Requirement
id: FR-061
---
Java と Kotlin の test の識別子は、`<拡張子を除いたファイルパス>:<型名を外側から内側の順に `/` で連結したパス>:<メソッド名>` の形式とする。package は識別子に含めない。

Kotlin のバッククォートで囲んだ名前は、バッククォートを除いた中身（空白を含んでよい）を名前とする。companion object は、名前があればその名前、なければ `Companion` を型名とする。Kotlin のトップレベル関数は、型名の部分を空にする。

例を 2つ示す。
- `src/test/kotlin/AuthTest.kt` の `class AuthTest` 内の `` fun `一覧 取得 は 200`() `` は、`src/test/kotlin/AuthTest:AuthTest:一覧 取得 は 200` になる。
- `class AuthTest { @Nested inner class 一覧取得 { @Test fun 取得() } }` 内の `取得` は、`src/test/kotlin/AuthTest:AuthTest/一覧取得:取得` になる。

Kotlin では、名前（バッククォートを除いた後）が空、または制御文字・U+0085・U+2028・U+2029 を含むテストは無視する。
