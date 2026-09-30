---
type: Requirement
id: FR-050
---
`judge --format json` は、`{"judgments":[{"edge": <edge id>, "kind": "doc-req"|"test-req", "verdict": "consistent"|"contradiction"|"uncertain", "score": <数値>|null, "reason": <文字列>|null, "by": "jev"|"llm"|null}]}` の形式で stdout に出力する。`judgments` の順序はエッジ ID の昇順とし、text 形式と同じエッジ集合・同じ順序を表す。`reason` は `consistent` のとき `null`、`score` と `by` は FR-047 の本文超過のとき `null` とする。
