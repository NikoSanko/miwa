---
type: Requirement
id: FR-042
---
`judge` は、Jev の応答 JSON の `answers.consistent.noul`（0 以上 1 以下の数値）を、そのエッジのスコアとする。次のいずれかの場合は、実行エラーとして exit 1 とする。

- 応答が JSON として解釈できない場合
- 当該フィールドが存在しない場合
- 当該フィールドが数値でない場合
- 値が 0 未満または 1 超の場合
