---
type: Requirement
id: FR-040
---
`judge` は実行開始時（最初の API 呼び出しの前）に、doc / req / test の本文と、索引対象ノードの要約（`description`）を `api.typesafe.ai` に渡す旨を stderr に1行で表示する。API キー未設定で終了する場合は、この表示は行わない。
