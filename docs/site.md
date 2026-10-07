---
type: Specification
id: DOC-SITE
title: "閲覧サイトを作る（site）"
description: "非開発者が GitHub なしで業務要件を読めるように、索引の木をそのまま静的 HTML 一式として出力する `miwa site` の動作と、`site` 設定、公開の例を説明する。"
---

# 閲覧サイトを作る（`site`）

## なぜサイトか

業務要件を確認したいのは開発者だけではありません。企画や運用のメンバーは GitHub のアカウントを持たないことが多く、リポジトリの Markdown を直接は読めません。そこで `miwa site` は、リポジトリの文書を **誰でもブラウザで読める静的な HTML 一式** にして出力します。

miwa の責務は、HTML 一式を出力するところまでです。どこに置くか、誰に見せるか（認証など）は miwa の外で決めます。

出力は [索引](index-command.md) の形をしています。各ページは「タイトル — 要約」のエントリ一覧か、文書の本文のどちらかで、読み手は索引のタイトルと要約をたどって読みたい文書にたどり着けます。

```
<out>/index.html（トップページ: 索引の根の一覧）
 ├─ docs/index.html（索引ページ: タイトル — 要約 の一覧）
 │   ├─ docs/setup.html（doc の本文）
 │   │                 └─ 本文中のリンク ──▶ req/xxx.html（req の本文）
 │   └─ ...
 └─ ...
```

検証の状態（drift など）は表示しません。サイトは「読むためのもの」で、検証は引き続き [`check`](check.md) の役割です。

## 使い方

```
miwa site [--out <dir>]
```

- [出力先は既定で `_site`（リポジトリルートからの相対）です。存在しなければ作り、空なら使います。空でない（ファイルでもフォルダでも 1 つでも入っている）ときや、フォルダではなくファイルとして存在するときは、何も書かずにエラーで止まります](../req/site-command-output-directory.md)。出力先を自動で削除しないのは、`--out .` のような指定で元の文書を消してしまう事故を防ぐためです。作り直したいときは、先に出力先を空にしてください。書き出しの途中で失敗したときは、部分的な出力が残ることがあります。そのときも出力先を空にしてからやり直してください。
- [`index` が未設定のときはエラーです。ロックは読まず、ネットワークも使いません。同じ入力からは同じ出力になります](../req/site-requires-index-and-is-deterministic.md)。サイトのナビゲーションを索引から作るためで、生成時刻なども埋め込まないので、差分で変化を確かめられます。

## 公開されるページ

[ページになるのは次のものです](../req/site-published-pages.md)。

- doc（`type: Specification`）と req（`type: Requirement`）
- 索引の根の配下で、索引の対象になる文書（`type` を持つ Markdown。例: `type: Reference`）
- 索引の根の配下の `index.md`（索引ページ）

`type` を持たない Markdown、`log.md`、除外されたフォルダのもの、Markdown 以外のファイルは公開しません。公開物に意図しないものを混ぜないためです（画像だけは後述のとおりコピーします）。フロントマターを解析できない文書もノードにならないので公開しません（索引ページは解析できなくても公開します）。同じファイルが複数の分類に当たっても、ページは 1 つです。

[出力先のパスは、元の `.md` を `.html` に置き換えたもの](../req/site-output-path-mirrors-source.md)です（`docs/a/b.md` は `<out>/docs/a/b.html`）。リポジトリのフォルダ構造をそのまま保つので、相対リンクが崩れません。

## 各ページの中身

[どのページも、外部の CSS・JavaScript・フォントを読み込まない 1 ファイルの HTML です](../req/site-page-structure.md)。外部へ画像を読みにいく先を `https:` に限り、参照元を送らない設定も入れています。ページ間のリンクはすべて相対パスです。

[ヘッダには、トップページと、そのページが属する最も近い索引ページへのリンクがあります](../req/site-header-links.md)。本文の先頭にタイトルと要約が並びます。本文の最初のブロックがレベル 1 の見出しのときは、[その見出しをページの見出しとして使い、同じタイトルを二重に出しません](../req/site-first-h1-is-not-duplicated.md)。doc と req のページには、タイトルの下に `id`（FR-098 のような番号）を表示するので、本文で番号を参照されても探せます。見出しには[ページ内リンク用の `id` が付きます](../req/site-heading-ids.md)。

元の Markdown は [UTF-8 として読み（不正なバイトは置換文字になります）、フロントマターは本文から取り除きます](../req/site-source-reading.md)。

[タイトルと要約は元のファイルから決めます](../req/site-page-title-and-description-sources.md)。doc・req・文書とも、[文書のタイトルの規則](../req/index-document-title-and-description.md)（`title`、無ければ本文の最初の見出し、無ければファイル名）と `description` を使います。req も `id` ではなくこの規則です。索引ページは、同じ要件に定めるディレクトリのタイトル・要約の規則に従います。

[本文は Markdown を HTML に変換したものです（表、取り消し線、タスクリスト、脚注に対応）。Markdown に書かれた生の HTML は、実行も描画もせず、文字のまま表示します](../req/site-markdown-to-html-escapes-raw-html.md)。HTML コメントだけは出力しません。公開するページにスクリプトなどが紛れ込まないようにするためです。

[req のページの末尾には、その req を参照している doc の一覧が付きます](../req/site-req-page-lists-referencing-docs.md)。業務要件の側から、どの技術要件に結びついているかをたどるのと逆に、技術要件から元の業務要件に戻れます。参照している doc が無ければ、この節は出ません。

## トップページと `site` 設定

[出力先の直下の `index.html` がトップページ](../req/site-top-page.md)です。タイトルと要約は `.miwa/config.yaml` の `site` で決めます。

```yaml
site:
  title: "○○プロジェクトの仕様"
  description: "業務要件の閲覧用サイト"
```

- [`site` は省略でき、`title` と `description` も省略できます。空の文字列と、改行を含む値は設定エラーです](../req/config-site-settings.md)。
- `title` を省略すると、miwa を実行したフォルダ（`.miwa/config.yaml` のあるフォルダ）の名前が使われます。公開するときは、読み手に伝わる名前を `site.title` に書くことをお勧めします。`description` を省略すると要約なしになります。前後の空白は無視されます。
- 本文は、`index` の各根を 1 エントリとして、`index` に書いた順に並べた一覧です。タイトルと要約は、その根の `index.md` のフロントマターの `title` / `description` から取ります。
- [根に `index.md` が無いときは、そのエントリをリンクなしの文字だけにして、標準エラーに警告を出します](../req/site-missing-root-index-warning.md)。

## リンクと画像の扱い

本文中のリンクは、公開後も辿れるように書き換えます。

- [`http:` `https:` `mailto:` と、ページ内の `#` だけのリンクはそのままです。相対リンクは `.html` を指すように書き換え、フォルダを指すリンクは、そこの索引ページへのリンクにします](../req/site-link-rewriting.md)。[リンク先のパスは、`?` 以降を捨て、パーセントデコードして解決し、出力するときに改めてエンコードします](../req/site-link-path-resolution.md)。
- [公開されないページへのリンクは、リンクを外して文字だけを残し、標準エラーに `warning: <元ファイル>: unpublished link: <リンク先>` の形で警告を 1 行 1 件出します](../req/site-unpublished-links-are-unlinked-with-warning.md)。`javascript:` などのスキーム、`/` で始まるリンク、リポジトリの外へ出るリンクも同じ扱いです。警告が出ても、終了コードは 0 です。壊れたリンクを公開物に残さないのと、直すべき箇所を作者に知らせるためです。
- [画像は、`docs` または `requirements` の配下にある通常のファイルで、png / jpg / jpeg / gif / webp（svg は、スクリプトを含み得るため対象外）ならサイトへコピーして表示します](../req/site-images-are-copied.md)。`https:` の画像だけはそのまま参照します（`http:` は対象外です）。それ以外は画像を出さず、代替テキストだけを残して警告を出します。

## 公開の例

出力された HTML を置く場所は miwa の外で決めます。一例として、GitHub Actions で `miwa site` を実行し、Cloudflare R2 の `<project>/` 配下へアップロードする流れがあります。

```
miwa site
aws s3 sync --delete _site/ s3://<bucket>/<project>/
```

`--delete` は、削除した文書をサイトからも消すためです。公開は CI などのクリーンなチェックアウトから行ってください。miwa は `.gitignore` を見ないので、手元で実行すると、未追跡のファイルや無視しているファイル（下書きなど）も公開物に入ります。閲覧者の認証（たとえば Cloudflare Access）は、配信側の責務で、miwa は関与しません。

出力先 `_site/` は生成物なので、`.gitignore` に `_site/` を足して、コミットしないようにしてください。

## まだ扱わないこと

検索、Mermaid 図の描画、検証状態の表示、Markdown 原文の同梱は、いまの `miwa site` では扱いません。
