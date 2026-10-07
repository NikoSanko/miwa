---
type: Requirement
id: FR-031
---
`check --diff [--base <ref>]` は、現在の HEAD と指定した参照（既定は設定の `diff_base`）の merge-base 時点の設定・ソース・ロックで評価した finding と、現在の finding を比較し、merge-base 時点には存在しなかった finding だけを報告する。
finding の同一性は「種別・対象の種類・対象・理由コード」で判定し、説明文の違いは同一性に影響しない。merge-base が取得できない場合は exit code 1 で終了する。
