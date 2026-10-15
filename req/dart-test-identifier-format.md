---
type: Requirement
id: FR-125
---
Dart / Flutter の test の識別子は `<.dart を除いたファイルパス>:<group 名を外側から内側の順に `/` で連結したパス>:<test 名>` の形式とする。group の外にある test では、group 部分を空文字列として連結する。

group 名・test 名は、文字列リテラルの引用符の内側のソース表記をそのまま使い、エスケープは解釈しない。raw 文字列（`r'...'` `r"..."`）も同様に扱う。隣接文字列連結（`'a' 'b'` など）は、各部分の引用符の内側を連結したものを名前とする。次の test / group は無視する。

- 名前が空文字列のもの
- 名前に改行・行区切り文字（U+2028 / U+2029 など）・制御文字を含むもの
- 名前に補間（`$x` / `${...}`）を含むもの
- 三重引用符（`'''` / `"""`）の文字列を名前に持つもの

例: `mobile/test/core/local_dev_test.dart` の `group('validateBackendUrl', () { test('rejects an empty backend URL', ...) })` は `mobile/test/core/local_dev_test:validateBackendUrl:rejects an empty backend URL`、group の外の `test('t', ...)` は `mobile/test/a_test::t`。
