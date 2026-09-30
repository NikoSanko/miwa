---
type: Requirement
id: FR-039
---
`judge` は環境変数 `TYPESAFE_API_KEY` と `ANTHROPIC_API_KEY` の両方を必要とする。どちらかが未設定または空文字列の場合、ネットワーク送信を一切行わず、未設定の変数名を stderr に表示して exit 1 とする。この確認は、対象エッジが 0 件の場合でも行う。
