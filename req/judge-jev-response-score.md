---
type: Requirement
id: FR-042
---
`judge` はJev の応答 JSON の `answers.consistent.noul`（0 以上 1 以下の数値）をそのエッジのスコアとする。応答が JSON として解釈できない場合、当該フィールドが存在しない・数値でない場合、または値が 0 未満もしくは 1 超の場合は、実行エラーとして exit 1 とする。
