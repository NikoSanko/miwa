---
type: Requirement
id: FR-046
---
`judge` は二次判定の応答の `content[0].text` を JSON として読み、`verdict`（`contradiction` または `uncertain`）と `reason` を当該エッジの結果とする（`by` は `llm`）。応答の `stop_reason` が `refusal` または `max_tokens` の場合、`content[0].text` が JSON として読めない場合、または `verdict` が `contradiction` / `uncertain` のいずれでもない場合は、当該エッジを `uncertain`、reason を「二次判定の応答を解釈できなかった」とし、実行は継続する（exit 1 にしない）。
