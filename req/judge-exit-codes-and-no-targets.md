---
type: Requirement
id: FR-051
---
`judge` は、すべての対象エッジの判定が完了した場合に exit 0 とする（`contradiction` や `uncertain` があっても exit 0）。設定エラー・API キー未設定・`--edge` の指定誤り・HTTP エラー・タイムアウト・一次判定の応答不正などの実行エラーは exit 1 とする。対象エッジが 0 件の場合は「対象なし」を表示して exit 0 とし、API は呼ばない（`--format json` では `{"judgments":[]}` を出力する）。
