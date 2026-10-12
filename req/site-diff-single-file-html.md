---
type: Requirement
id: FR-114
---
`miwa site --diff` は、全体を 1 つの HTML5 ファイルに出力する。`<html lang="ja">`、`<meta charset="utf-8">`、viewport の `<meta>`、`<title>`、`<meta name="referrer" content="no-referrer">` を持つ。`<title>` は `<base ref> → <head label> — <site のタイトル>` とする（各値は FR-115 のヘッダと同じ。site のタイトルは FR-093 と同じ規則で決める）。

`<meta http-equiv="Content-Security-Policy" content="default-src 'none'; img-src 'self' data: https:; style-src 'unsafe-inline'; base-uri 'none'; form-action 'none'">` を入れる。FR-095 の CSP に対して、画像の埋め込み（FR-120）のため `img-src` に `data:` を足したものである。スタイルはインライン CSS だけで与え、JavaScript は含めない。開閉は `<details>` と CSS だけで行う。外部の CSS・JavaScript・フォントは読み込まない。`https:` の画像だけは `miwa site` と同じくそのまま参照する（表示にはネットワークが要る）。

Markdown の生の HTML は、FR-097 と同じく実行も描画もせず、文字のまま表示する。HTML コメントは出力しない。タイトル、パス、ラベルなど、テンプレートに埋め込む文字列は、すべて HTML エスケープする。

生成時刻などの実行ごとに変わる値は埋め込まず、同じ入力からは同じバイト列を出力する。未公開リンクの警告（FR-099）と未公開画像の警告（FR-100）は出さない。req ページの末尾の、参照している doc の一覧（FR-101）は出さない。

ライト / ダークの両方に対応し（`prefers-color-scheme`）、狭い画面では左右を縦に積む。縦に積むときは、変わっていないブロックを片側だけ出す。
