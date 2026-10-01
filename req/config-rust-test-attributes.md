---
type: Requirement
id: FR-058
---
`.miwa/config.yaml` の `rust.test_attributes` は省略可能とし、省略時は空の一覧として扱う（`rust:` のみ、または `rust: {}` も空として扱う）。各エントリは `[A-Za-z0-9_]` のみからなる非空の文字列でなければならず、違反する場合は設定エラーとして実行を停止する（exit 1）。エラーメッセージは、属性パスの末尾セグメントのみを指定する旨（例: `rstest` であり `rstest::rstest` ではない）を示す。
