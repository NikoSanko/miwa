---
type: Requirement
id: FR-095
---
すべてのページは HTML5 で、`<html lang="ja">`、`<meta charset="utf-8">`、viewport の `<meta>`、`<title>` を持つ。さらに、`<meta http-equiv="Content-Security-Policy" content="default-src 'none'; img-src 'self' https:; style-src 'unsafe-inline'; base-uri 'none'; form-action 'none'">` と `<meta name="referrer" content="no-referrer">` を入れる（公開物から外部への読み込みや参照元の送信を抑えるため）。スタイルはページ内のインライン CSS だけで与える。外部の CSS・JavaScript・フォントは読み込まず、JavaScript も含めない。

本文の先頭には `<h1>` でタイトルを置く（本文の最初のブロックがレベル 1 見出しのときは、FR-105 に従いその見出しを使う）。その直後に、要約を `<p class="description">要約</p>` の形の段落で表示する（本文の段落と区別するため）。要約が無ければ、段落を出さない。要約があるときは、`<meta name="description">` にも同じ値を入れる。タイトル・要約などテンプレートへ埋め込む文字列は、すべて HTML エスケープする。

miwa が出す内部リンク（ヘッダ、トップページの一覧、参照元の一覧、本文中のリンク）は、すべてそのページからの相対パスとし、`index.html` などのファイル名まで含める。`/` で始まる URL は出さない。href の符号化は FR-107 に従う（トップページの一覧、参照元の一覧、ヘッダも対象）。

doc ノードと req ノードのページには、`<h1>`（FR-105 により本文の見出しのときはその見出し）の直後、要約の前に、`<p class="id">FR-098</p>` の形で `id` を表示する（本文中で FR 番号を参照されても探せるようにするため）。索引対象文書（ノードでないもの）と索引ページには出さない。
