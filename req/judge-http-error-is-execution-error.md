---
type: Requirement
id: FR-043
---
`judge` が呼ぶ API（TypeSafe / Anthropic のいずれも）が HTTP 4xx / 5xx を返した場合、またはタイムアウトした場合は、実行エラーとして exit 1 とする。一次判定（Jev）の失敗時に LLM のみで判定するフォールバックは行わない。
