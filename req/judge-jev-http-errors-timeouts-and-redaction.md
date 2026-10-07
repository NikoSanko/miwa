---
type: Requirement
id: FR-043
---
Jev の呼び出し（`api.typesafe.ai`）が HTTP 4xx / 5xx を返した場合、またはタイムアウトした場合は、`judge` は実行エラーとして exit 1 とする。HTTP 呼び出しのタイムアウトは、接続が 10 秒、全体が 60 秒とする。
エラー出力（stderr・エラーメッセージ）には、API キーの値および Authorization ヘッダの値を含めない。
