---
type: Requirement
id: FR-014
---
Go・Rust・C#・Java・Kotlin・TypeScript / JavaScript のいずれの言語でも、1つのテスト（関数・メソッド、TypeScript / JavaScript では呼び出し文）に隣接するマーカーが2件以上ある場合、そのテストは `invalid` として報告される。また、対象のテストファイル（`*_test.go` / `*.rs` / `*.cs` / `*.java` / `*.kt` / `*.test.<ext>` / `*.spec.<ext>`、ext は FR-053 の 8 種）が構文エラーで解析できない場合も、そのファイルは `invalid` として報告される。
