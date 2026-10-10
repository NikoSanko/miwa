---
type: BusinessNeed
id: DOC-SITE
title: "閲覧サイトを作る（site）"
description: "非開発者が GitHub なしで業務要求を読めるように、索引の木をそのまま静的 HTML 一式として出力する `miwa site` の動作と、`site` 設定、公開の例を説明する。"
---

# 閲覧サイトを作る（`site`）

## なぜサイトか

業務要求を確認したいのは、開発者だけではありません。企画や運用のメンバーは GitHub のアカウントを持たないことが多く、リポジトリの Markdown を直接は読めません。そこで `miwa site` は、リポジトリの文書を **誰でもブラウザで読める静的な HTML 一式** にして出力します。

miwa の責務は、HTML 一式を出力するところまでです。置き場所と、誰に見せるか（認証など）は miwa の外で決めます。

出力は [索引](index-command.md) の形をしています。各ページは、「タイトル — 要約」のエントリ一覧か、文書の本文のどちらかです。読み手は索引のタイトルと要約をたどって、読みたい文書にたどり着けます。

```
<out>/index.html（トップページ: 索引の根の一覧）
 ├─ docs/index.html（索引ページ: タイトル — 要約 の一覧）
 │   ├─ docs/setup.html（doc の本文）
 │   │                 └─ 本文中のリンク ──▶ req/xxx.html（req の本文）
 │   └─ ...
 └─ ...
```

検証の状態（drift など）は表示しません。サイトは読むためのもので、検証は引き続き [`check`](check.md) の役割です。

## 使い方

```
miwa site [--out <dir>]
```

- [出力先は既定で `_site`（リポジトリルートからの相対）です](../req/site-command-output-directory.md)。存在しなければ作り、空なら使います。空でないときと、フォルダではなくファイルとして存在するときは、何も書かずにエラーで止まります。空でないとは、ファイルでもフォルダでも1つでも入っている場合です。出力先を自動で削除しないのは、`--out .` のような指定で元の文書を消す事故を防ぐためです。作り直したいときは、先に出力先を空にしてください。書き出しの途中で失敗したときは、部分的な出力が残ることがあります。そのときも、出力先を空にしてからやり直してください。
- [`index` が未設定のときはエラーです](../req/site-requires-index-and-is-deterministic.md)。サイトのナビゲーションを索引から作るためです。ロックは読まず、ネットワークも使いません。同じ入力からは同じ出力になります。生成時刻なども埋め込まないので、差分で変化を確かめられます。

## 公開されるページ

[ページになるのは次のものです](../req/site-published-pages.md)。

- doc（`type: BusinessNeed`）と req（`type: Requirement`）
- 索引の根の配下で、索引の対象になる文書（`type` を持つ Markdown。例: `type: Reference`）
- 索引の根の配下の `index.md`（索引ページ）

次のものは、公開物に意図しないものを混ぜないため公開しません。

- `type` を持たない Markdown
- `log.md`
- 除外されたフォルダのもの
- Markdown 以外のファイル（画像だけは、後述のとおりコピーします）

フロントマターを解析できない文書も、ノードにならないため公開しません。ただし索引ページは、解析できなくても公開します。

同じファイルが複数の分類に当たっても、ページは1つです。

[出力先のパスは、元の `.md` を `.html` に置き換えたもの](../req/site-output-path-mirrors-source.md)です（`docs/a/b.md` は `<out>/docs/a/b.html`）。リポジトリのフォルダ構造をそのまま保つので、相対リンクが崩れません。

## 各ページの中身

[どのページも、1 ファイルの HTML です](../req/site-page-structure.md)。外部の CSS・JavaScript・フォントは読み込みません。外部の画像を読みにいく先は `https:` に限り、参照元を送らない設定も入れています。ページ間のリンクはすべて相対パスです。

[ヘッダには、トップページと、そのページが属する最も近い索引ページへのリンクがあります](../req/site-header-links.md)。本文の先頭には、タイトルと要約が並びます。本文の最初のブロックがレベル 1 の見出しのときは、[その見出しをページの見出しとして使います](../req/site-first-h1-is-not-duplicated.md)。同じタイトルは二重に出しません。

doc と req のページには、タイトルの下に `id`（FR-098 のような番号）を表示します。本文で番号を参照されても探せるようにするためです。見出しには[ページ内リンク用の `id` が付きます](../req/site-heading-ids.md)。

元の Markdown は、[UTF-8 として読みます](../req/site-source-reading.md)。不正なバイトは置換文字になります。フロントマターは本文から取り除きます。

[タイトルと要約は元のファイルから決めます](../req/site-page-title-and-description-sources.md)。doc・req・文書とも、[文書のタイトルの規則](../req/index-document-title-and-description.md)と `description` を使います。タイトルの規則は、`title`、無ければ本文の最初の見出し、それも無ければファイル名の順です。req も `id` ではなく、この規則に従います。索引ページは、同じ要件に定めるディレクトリのタイトル・要約の規則に従います。

[本文は Markdown を HTML に変換したものです](../req/site-markdown-to-html-escapes-raw-html.md)。表、取り消し線、タスクリスト、脚注に対応します。Markdown に書かれた生の HTML は、公開するページにスクリプトなどが紛れ込まないように、実行も描画もせず、文字のまま表示します。HTML コメントだけは出力しません。

[req のページの末尾には、その req を参照している doc の一覧が付きます](../req/site-req-page-lists-referencing-docs.md)。業務要求の側から結びついたシステム要件をたどるのとは逆に、システム要件から元の業務要求へ戻れます。参照している doc が無ければ、この節は出ません。

## トップページと `site` 設定

[出力先の直下の `index.html` がトップページ](../req/site-top-page.md)です。タイトルと要約は `.miwa/config.yaml` の `site` で決めます。

```yaml
site:
  title: "○○プロジェクトの仕様"
  description: "業務要求の閲覧用サイト"
```

- [`site` は省略でき、`title` と `description` も省略できます](../req/config-site-settings.md)。空の文字列と、改行を含む値は設定エラーです。
- `title` を省略すると、miwa を実行したフォルダ（`.miwa/config.yaml` のあるフォルダ）の名前が使われます。公開するときは、読み手に伝わる名前を `site.title` に書くことをお勧めします。`description` を省略すると、要約なしになります。前後の空白は無視されます。
- 本文は、`index` の各根を 1 エントリとして、`index` に書いた順に並べた一覧です。タイトルと要約は、その根の `index.md` のフロントマターの `title` / `description` から取ります。
- [根に `index.md` が無いときは](../req/site-missing-root-index-warning.md)、そのエントリをリンクなしの文字だけにします。標準エラーには警告を出します。

## リンクと画像の扱い

本文中のリンクは、公開後も辿れるように書き換えます。

- [`http:` `https:` `mailto:` と、ページ内の `#` だけのリンクはそのままです](../req/site-link-rewriting.md)。相対リンクは `.html` を指すように書き換えます。フォルダを指すリンクは、そこの索引ページへのリンクにします。リンク先のパスは、`?` 以降を捨て、パーセントデコードして解決し、出力するときに改めてエンコードします（[パスの解決規則](../req/site-link-path-resolution.md)）。
- [公開されないページへのリンクは、リンクを外して文字だけを残します](../req/site-unpublished-links-are-unlinked-with-warning.md)。標準エラーには、`warning: <元ファイル>: unpublished link: <リンク先>` の形で警告を1行1件出します。壊れたリンクを公開物に残さないことと、直すべき箇所を作者に知らせることが目的です。`javascript:` などのスキーム、`/` で始まるリンク、リポジトリの外へ出るリンクも同じ扱いです。警告が出ても、終了コードは 0 です。
- [画像は、条件を満たすものだけサイトへコピーして表示します](../req/site-images-are-copied.md)。条件は、`docs` または `requirements` の配下にある通常のファイルで、png / jpg / jpeg / gif / webp のいずれかであることです。svg は、スクリプトを含み得るため対象外です。`https:` の画像だけは、そのまま参照します（`http:` は対象外です）。それ以外は画像を出さず、代替テキストだけを残して警告を出します。

## 公開の例

出力された HTML を置く場所は、miwa の外で決めます。一例として、GitHub Actions で `miwa site` を実行し、Cloudflare R2 の `<project>/` 配下へアップロードする流れがあります。

```
miwa site
aws s3 sync --delete _site/ s3://<bucket>/<project>/
```

`--delete` は、削除した文書をサイトからも消すためです。公開は CI などのクリーンなチェックアウトから行ってください。miwa は `.gitignore` を見ないので、手元で実行すると、未追跡のファイルや無視しているファイル（下書きなど）も公開物に入ります。閲覧者の認証（例えば Cloudflare Access）は配信側の責務で、miwa は関与しません。

出力先の `_site/` は生成物なので、`.gitignore` に `_site/` を足して、コミットしないようにしてください。

## まだ扱わないこと

検索、Mermaid 図の描画、検証状態の表示、Markdown 原文の同梱は、いまの `miwa site` では扱いません。
