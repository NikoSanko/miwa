---
type: Requirement
id: FR-014
---
Go・Rust・C#・Java・Kotlin・TypeScript / JavaScript のいずれの言語でも、1つのテスト（関数・メソッド、TypeScript / JavaScript では呼び出し文）に隣接するマーカーが2件以上ある場合、そのテストは `invalid` として報告される。

また、対象のテストファイルが構文エラーで解析できない場合も、そのファイルは `invalid` として報告される。対象のテストファイルは、`*_test.go` / `*.rs` / `*.cs` / `*.java` / `*.kt` / `*.test.<ext>` / `*.spec.<ext>` である（ext は FR-053 の8種）。
