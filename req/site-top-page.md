---
type: Requirement
id: FR-093
---
`miwa site` は、出力先の直下に `index.html`（トップページ）を作る。`<title>` と `<h1>` は、設定 `site.title` とする。省略時は、miwa を実行したディレクトリ、つまり `.miwa/config.yaml` のあるディレクトリの名前とする。`<meta name="description">` と `<h1>` 直下の要約は、`site.description` とする。省略時は、どちらも出さない。

本文は、`index` の各根を 1 エントリとする一覧とする。並びは `index` 設定に書かれた順とし、`<ul><li><a href="docs/index.html">タイトル</a> — 要約</li></ul>` の形で HTML を直接組む（要約が無いエントリは ` — 要約` を付けない）。href は、トップページからの相対パスでファイル名まで含める。

タイトルと要約は、その根の `index.md` のフロントマターの `title` / `description` で決める。規則は FR-071 のディレクトリと同じである（`title` が無い・空・改行を含むときはディレクトリ名、`description` が無い・空・改行を含むときは要約なし）。根に `index.md` が無いときは、FR-103 に従う。
