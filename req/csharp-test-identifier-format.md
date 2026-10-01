---
type: Requirement
id: FR-051
---
C# の test の識別子は `<拡張子を除いたファイルパス>:<型名を外側から内側の順に `/` で連結したパス>:<メソッド名>` の形式とする。namespace は識別子に含めず、`class` `struct` `record` `interface` の入れ子を外側から辿って連結する。例: `tests/AuthTests.cs` の `class AuthTests` 内の `RejectsShort` は `tests/AuthTests:AuthTests:RejectsShort`、`class Outer { class Inner { void T3() } }` 内の `T3` は `tests/X:Outer/Inner:T3`。
