---
type: BusinessNeed
id: DOC-SITE
title: "閲覧サイトを作る（site）"
description: "非開発者が GitHub なしで業務要求を読めるように、索引の木をそのまま静的 HTML 一式として出力する `miwa site` の動作と、`site` 設定、PR の差分を確認する `--diff`、公開の例を説明する。"
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

## 差分を確認する（`--diff`）

PR のレビューでは、Files changed に Markdown の生の差分しか出ません。描画された文書として何が変わったかは読み取りにくくなります。`miwa site --diff` は、[比較の分岐点から現在までに変わった文書だけを、1 ファイルの HTML に出力します](../req/site-diff-command-and-base.md)。GitHub の split 表示のように、左が変更前、右が変更後です。

### 使い方

```
miwa site --diff [--base <ref>] [--out <file>] [--link-base <url>] [--head-label <text>]
```

- 比較は [`check --diff`](check.md) と同じです。設定の `diff_base`（`--base` で上書きできます）と HEAD の merge-base を変更前、作業ツリーを変更後とします。未コミットや未追跡のファイルも変更後に入ります。merge-base を特定できないときは、終了コード 1 のエラーです。
- [`--base` `--link-base` `--head-label` は `--diff` と一緒にしか指定できません](../req/site-diff-command-and-base.md)。ロックは読まず、ネットワークも使いません。標準出力には何も出しません。
- [`--out` はディレクトリではなくファイルのパスで、既定は `_site-diff.html` です](../req/site-diff-output-file.md)。出力先に何かが既にあるときは、種類を問わず、何も書かずに終了コード 1 で止まります。比較より前に確認するので、変更がない場合も同じです。親フォルダが無ければ作ります。
- [`--head-label` はヘッダに出す変更後の名前で、表示専用です](../req/site-diff-header-and-tree.md)。省略すると現在のブランチ名、detached HEAD なら `HEAD` になります。CI は PR の head のコミットを checkout して detached になるため、ブランチ名を渡します。改行や制御文字を含む値はエラーです。
- [対象の変更が 1 件もないときは、HTML を作らず、終了コード 0 で終わります](../req/site-diff-no-changes.md)。CI では、ファイルがあるかどうかで次の処理を決められます。

### 差分の対象

[公開されるページの範囲は `miwa site` と同じです](../req/site-diff-target-documents.md)。変更前と変更後は、それぞれの時点の設定で評価します。変更後の設定に `index` が無いときはエラーです。変更前の時点に設定が無ければ、変更前の公開範囲は空として扱い、全文書が追加になります。変更前の設定に `index` が無ければ、doc と req だけが変更前の範囲です。

次のいずれかが変わった文書が、差分の対象です。変わっていない文書は含めません。

- 追加、削除、パスの変更（移動）
- 本文
- `title` または `description`（本文が同じでも対象です）
- 本文から参照している画像ファイルの中身（差し替え）

[索引ページ（`index.md`）](../req/site-diff-index-pages-by-path.md)も、元の Markdown が変わったり、追加・削除されたりしたときに対象です。索引ページは文書ではないため、パスで対応づけます。ディレクトリを移動したときは、旧パスの削除と新パスの追加として出ます。

### 変更前と変更後の対応づけ

[文書は、（分類, `id`）が同じものを同じ文書として対応づけます](../req/site-diff-pairing-by-id.md)。分類は doc（`BusinessNeed`）、req（`Requirement`）、それ以外の `type` を持つ文書の 3 つです。分類が違えば `id` が同じでも別の文書で、`type` を分類をまたいで変えた文書は、削除と追加になります。`id` が同じでパスが違えば、移動として「旧パス → 新パス」と本文の差分を出します。

少なくとも一方の側で `id` を持たない文書は、反対側の、同じパスで同じ分類の文書と対にします。git のリネーム検出は使いません。

変更後の側に `id` が無い文書が差分の対象になるときは、何も書かずに終了コード 1 で止まります。対象になるのは、追加された文書と、本文・`title`・`description`・画像が変わった文書です。変更前にあった `id` を消した文書も同じです。該当する文書は全件を、次の形で標準エラーに出します。

```
documents in the diff have no id; add "id:" to their front matter:
docs/example.md
req/example.md
```

- 変更前の側だけで `id` が無い文書は、PR では直せないのでエラーにしません。同じパスの相手が無ければ、削除として出ます。
- 変わっていない文書に `id` が無くても、エラーにしません。`id` を足すだけの変更は、内容が変わらないので対象外で、エラーにもなりません。`id` の追加と本文の変更を同時にしたときは、同じパスで対になり、変更として出ます。
- `type: Reference` のような doc と req 以外の文書は、`id` が任意です。導入先でこのエラーに当たったときは、表示されたファイルに `id` を足してください。
- この検査には、索引の根の外にある `docs` と `requirements` の配下の doc と req も含めます。`id` を消した req は、削除ではなくエラーになります。
- 同じ側に、同じ分類で同じ `id` の文書が複数あるときは、差分の対象かどうかにかかわらず終了コード 1 です。

### 表示

[1 ファイルの HTML で、JavaScript は使いません](../req/site-diff-single-file-html.md)。開閉は `<details>` と CSS だけで行います。`site` と同じく、スクリプトを禁止した CSP の下で動き、CSS・JavaScript・フォントなどの外部リソースは読みません。ライトとダークの両方に対応し、狭い画面では左右を縦に積みます。

[ヘッダには、サイトのタイトルと、比較の2点を `main (3460f94) → feature/x (c0ffee1)` の形で1行に出します](../req/site-diff-header-and-tree.md)。右端に状態ごとの件数を出します。状態のアイコンは、変更が `•`、追加が `+`、削除が `−`、移動が `→` です。左には、変わった文書をディレクトリの木にしたファイルツリーがあり、アイコンを付け、クリックでその文書へ移動します。狭い画面では隠れます。

[文書ごとの枠の見出し行には、状態アイコン、タイトル、`id`、パス、変更量のバー（5つの四角）が並びます](../req/site-diff-document-block.md)。見出し行はスクロールしても上部に残り、枠ごと畳めます。タイトルが変わったときは旧タイトルを取り消し線で並べ、`title` と `description` の変更は、本文より前の専用の行で見せます。移動しただけの文書は「内容の変更なし」の1行だけです。追加と削除された文書は、左右に分けず1列で出します。

[左右は、ブロック（段落、見出し、コードブロック、表、リストの項目など）の単位で対応づけます](../req/site-diff-block-alignment.md)。変わったブロックは左を赤、右を緑にし、端に `−` と `+` を付けます。片側にしか無いブロックの反対側は斜線です。変わったブロックの中は、文字単位で変わった部分を濃い色で塗ります。日本語は空白で区切れないため、単語ではなく文字を単位にしています。計算量には決定的な上限があり、超えたときは文字の強調なし、または全削除と全追加として出します。時間で打ち切ることはしないので、出力は毎回同じです。

[変わったブロックの前後1ブロックだけを表示し、残りは `↕ 12 § 設計方針` のような1行に畳みます](../req/site-diff-context-folding.md)。数字は畳んだブロックの数、`§` の後ろはその範囲の最後の見出しです。押すと開き、閉じ直すことはできません。

脚注は番号ではなくラベルで表示します。req ページ末尾の参照元の一覧は、文書の内容ではないので出しません。未公開リンクの警告も、`site` と `check` の役割なので出しません。

### リンクと画像

[本文中のリンクは次のように扱います](../req/site-diff-links.md)。

- 差分の HTML に含まれる文書へのリンクは、ページ内のアンカーにします。
- 含まれない文書へのリンクは、`--link-base` と元の `.md` のパスをつないだ URL にします。例は `--link-base https://github.com/<owner>/<repo>/blob/<HEAD の SHA>/` です。`--link-base` は `http:` か `https:` で始まる URL に限り、それ以外はエラーです。末尾に `/` が無ければ補います。
- `--link-base` が無いときと、公開されないページへのリンクは、リンクを外して文字だけを残します。
- `http:` `https:` `mailto:` と `#` だけのリンクは、そのままです。

[画像は `data:` URI で埋め込み、ダウンロードした HTML をそのまま開けるようにします](../req/site-diff-images-inlined.md)。埋め込む条件は `site` と同じです（png / jpg / jpeg / gif / webp。svg は対象外）。画像を差し替えたブロックは、変わったブロックとして左右に並べます。`https:` の画像は `site` と同じく参照先のままなので、表示にはネットワークが必要です。

### GitHub Actions での使い方

PR ごとに差分 HTML を作り、Workflow の Artifact にアップロードして、PR のコメントからリンクする例です。コメントは PR につき1件で、push のたびに更新します。変更がなければ Artifact は作らず、コメントに「変更なし」と書きます。fork からの PR では、コメントの書き込み権限が無いため、コメントを省略して Artifact だけ作ります。`pull_request_target` は、信頼できないコードを書き込み権限で動かすことになるので使いません。

miwa は、`cargo install --git https://github.com/NikoSanko/miwa --tag vX.Y.Z --locked` で入れるか、Releases のバイナリをダウンロードして使います。次の例は `cargo install` を使います。バージョンは、プロジェクトで使うものに置き換えてください。

```yaml
name: site-diff

on: pull_request

permissions:
  contents: read
  pull-requests: write

concurrency:
  group: site-diff-${{ github.event.pull_request.number }}
  cancel-in-progress: true

jobs:
  site-diff:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
        with:
          ref: ${{ github.event.pull_request.head.sha }}
          fetch-depth: 0

      - uses: dtolnay/rust-toolchain@stable

      - name: Install miwa
        run: cargo install --git https://github.com/NikoSanko/miwa --tag vX.Y.Z --locked

      - name: Build the diff
        env:
          BASE_REF: ${{ github.base_ref }}
          HEAD_REF: ${{ github.head_ref }}
          HEAD_SHA: ${{ github.event.pull_request.head.sha }}
          REPO: ${{ github.repository }}
        run: |
          miwa site --diff \
            --base "origin/${BASE_REF}" \
            --head-label "${HEAD_REF}" \
            --out _site-diff.html \
            --link-base "https://github.com/${REPO}/blob/${HEAD_SHA}/"

      - name: Check the output
        id: output
        run: |
          if [ -f _site-diff.html ]; then
            echo "exists=true" >> "$GITHUB_OUTPUT"
          else
            echo "exists=false" >> "$GITHUB_OUTPUT"
          fi

      - name: Upload the diff
        id: upload
        if: steps.output.outputs.exists == 'true'
        uses: actions/upload-artifact@v7
        with:
          path: _site-diff.html
          archive: false
          retention-days: 14

      - name: Comment on the PR
        if: github.event.pull_request.head.repo.full_name == github.repository
        uses: actions/github-script@v7
        env:
          EXISTS: ${{ steps.output.outputs.exists }}
          ARTIFACT_URL: ${{ steps.upload.outputs.artifact-url }}
          HEAD_SHA: ${{ github.event.pull_request.head.sha }}
        with:
          script: |
            const marker = '<!-- miwa-site-diff -->';
            const short = process.env.HEAD_SHA.slice(0, 7);
            const body = process.env.EXISTS === 'true'
              ? `${marker}\n[差分 HTML を開く](${process.env.ARTIFACT_URL}) (${short})`
              : `${marker}\n変更なし (${short})`;
            const { owner, repo } = context.repo;
            const issue_number = context.payload.pull_request.number;
            const comments = await github.paginate(
              github.rest.issues.listComments,
              { owner, repo, issue_number, per_page: 100 },
            );
            const existing = comments.find(
              (c) => c.user.login === 'github-actions[bot]' && c.body.includes(marker),
            );
            if (existing) {
              await github.rest.issues.updateComment({ owner, repo, comment_id: existing.id, body });
            } else {
              await github.rest.issues.createComment({ owner, repo, issue_number, body });
            }
```

- GitHub の式（`${{ ... }}`）は、ブランチ名などの利用者が決める値を含むため、`run:` に直接書かず、`env:` で環境変数にして渡します。シェルの文字列に展開されて、コマンドとして解釈されるのを防ぐためです。
- チェックアウトは PR の head のコミットにして、`fetch-depth: 0` で履歴をすべて取ります。merge-base を特定するためです。浅いクローンでは、終了コード 1 で止まります。
- Artifact の URL は、GitHub にログインしてリポジトリにアクセスできる人だけが開けます。リポジトリにアクセスできない人には共有できません。`archive: false` を指定すると、HTML が zip にならずそのまま保存されます。このとき `name` は使われず、ファイル名（`_site-diff.html`）が Artifact の名前になります。
- コメントは、目印の `<!-- miwa-site-diff -->` を持ち、作者が `github-actions[bot]` のものを探して更新します。ほかの人のコメントは書き換えません。
- 保存期間（`retention-days`）はプロジェクトで決めてください。
- 差分の対象に `id` の無い文書があると、`miwa site --diff` が終了コード 1 で止まり、Workflow は失敗します。メッセージに出た文書へ `id` を足してください。

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
