---
type: Requirement
id: FR-039
---
`judge` は環境変数 `TYPESAFE_API_KEY` を必要とする。未設定または空文字列の場合は、ネットワーク送信を一切行わず、変数名を stderr に表示して exit 1 とする。この確認は、対象エッジが0件の場合でも行う。
