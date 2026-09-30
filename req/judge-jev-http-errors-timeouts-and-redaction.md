---
type: Requirement
id: FR-043
---
一次判定（`api.typesafe.ai`）が HTTP 4xx / 5xx を返した場合、またはタイムアウトした場合は、`judge` は実行エラーとして exit 1 とする（エージェントへのフォールバックはしない）。HTTP 呼び出しは、接続タイムアウト 10 秒・全体タイムアウト 60 秒とする。エラー出力（stderr・エラーメッセージ）には、API キーの値および Authorization ヘッダの値を含めない。
