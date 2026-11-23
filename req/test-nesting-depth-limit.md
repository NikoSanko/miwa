---
type: Requirement
id: FR-127
---
テストを探すために降りる入れ子は、最上位から数えて 256 段までとする。入れ子として数えるのは、Rust の inline `mod`、C# の名前空間・型宣言・`#if` / `#elif` / `#else`、Java の型宣言、Kotlin の型宣言（`object` と `companion object` を含む）、TypeScript / JavaScript の `describe`、Dart の `group` である。

入れ子が 256 段を超えるテストファイルは、構文エラーの場合（FR-014）と同じく、ファイル全体を `invalid` として報告し、そのファイルからは test を抽出しない。入れ子がどれほど深くても、`check` などのコマンドがスタックオーバーフローで異常終了することはない。
