---
type: Requirement
id: FR-047
---
`judge` は、全エッジの判定が完了した場合に exit 0 とする（`review` や `skipped` があっても exit 0）。設定エラー・API キー未設定・`--edge` の指定誤り・Jev の HTTP エラーやタイムアウト・Jev の応答不正は実行エラーとして exit 1 とする。対象エッジが 0 件の場合は「対象なし」を表示して exit 0 とし、Jev を呼ばない。
