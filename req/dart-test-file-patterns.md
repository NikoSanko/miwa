---
type: Requirement
id: FR-122
---
Dart / Flutter（`package:test` / `flutter_test`）の test の抽出対象ファイルは、`tests` に設定されたディレクトリ配下の `*_test.dart` とする。`_test.dart` で終わらない `.dart` ファイル（ヘルパーや `lib` 配下のコードなど）は対象外とする。

ファイルは Dart 構文で解析し、構文エラーがあれば invalid とする（FR-014）。
