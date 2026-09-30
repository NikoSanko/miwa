---
type: Requirement
id: FR-030
---
`check --gate` は、finding が1件以上あれば exit code 2 を返す。設定エラーやロック破損等の実行エラーが発生した場合は exit code 1 を返す。finding が0件で実行エラーもない場合は exit code 0 を返す。`--gate` を付けない `check` は、finding の有無にかかわらず exit code 0 を返す（実行エラー時は 1）。
