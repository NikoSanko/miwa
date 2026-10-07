---
type: Requirement
id: FR-103
---
`index` の根に `index.md` が無いとき、トップページのその根のエントリは `<a>` を付けないタイトルの文字だけにし（タイトルはディレクトリ名）、`warning: <根>/index.md: missing index` を標準エラーへ出す。終了コードは 0 のままである。この警告は、ページに関する警告より前に、`index` 設定の順で出す（FR-099）。
