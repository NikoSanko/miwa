---
type: Requirement
id: FR-040
---
`judge` は実行開始時（最初の API 呼び出しの前）に、doc / req / test の本文を `api.typesafe.ai` と `api.anthropic.com` に送信する旨を stderr に1行で表示する。この表示は stdout に出力しない（`--format json` の出力を汚さない）。API キー未設定で終了する場合は、この表示は行わない。
