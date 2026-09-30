---
type: Requirement
id: FR-045
---
二次判定として `judge` は `POST https://api.anthropic.com/v1/messages` を1回呼ぶ。ヘッダは `x-api-key: <ANTHROPIC_API_KEY>`、`anthropic-version: 2023-06-01`、`Content-Type: application/json`。本体は `model`（設定 `judge.llm_model` の値）、`max_tokens: 1024`、`output_config.format`、`messages` を持ち、`thinking` は含めない。`output_config.format` は `{ "type": "json_schema", "schema": { "type": "object", "properties": { "verdict": { "type": "string", "enum": ["contradiction", "uncertain"] }, "reason": { "type": "string" } }, "required": ["verdict", "reason"], "additionalProperties": false } }`。`messages` は role が `user` の1件で、その内容にエッジ種別（doc–req / test–req）、両端の本文、「矛盾していれば contradiction、判断材料が足りなければ uncertain とし、理由を述べる」旨の指示を含む。
