---
type: Requirement
id: FR-003
---
test のハッシュ計算対象は、Go・Rust・C#・Java・Kotlin では関数（メソッド）定義全体（シグネチャと本体）、TypeScript / JavaScript では `test(...)` / `it(...)` の呼び出し文全体とする。いずれの言語でも、本体内の1文字の変更がハッシュ値の変化として検出される。
