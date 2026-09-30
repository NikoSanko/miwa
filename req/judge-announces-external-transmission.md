---
type: Requirement
id: FR-040
---
`judge` は実行開始時（最初の API 呼び出しの前）に、doc / req / test の本文を `api.typesafe.ai` と、設定された二次判定のエージェント CLI（`judge.llm_command`）に渡す旨を stderr に1行で表示する。API キー未設定で終了する場合は、この表示は行わない。
