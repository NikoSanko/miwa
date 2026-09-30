---
type: Requirement
id: FR-017
---
Rust の test の識別子は `<拡張子を除いたファイルパス>:<inline mod を外側から内側の順に `/` で連結したパス>:<関数名>` の形式とする。inline mod に属さないトップレベルの test 関数では、モジュールパス部分は空文字列として連結する。例: `src/lock.rs` のトップレベルの `fn roundtrip` は `src/lock::roundtrip`、`mod tests { fn roundtrip }` 内なら `src/lock:tests:roundtrip`。
