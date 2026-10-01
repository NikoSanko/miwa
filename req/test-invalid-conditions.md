---
type: Requirement
id: FR-014
---
Go・Rust・C#・TypeScript のいずれの言語でも、1つのテスト（関数・メソッド、TypeScript では呼び出し文）に隣接するマーカーが2件以上ある場合、そのテストは `invalid` として報告される。また、対象のテストファイル（`*_test.go` / `*.rs` / `*.cs` / `*.test.ts` `*.spec.ts` `*.test.tsx` `*.spec.tsx`）が構文エラーで解析できない場合も、そのファイルは `invalid` として報告される。
