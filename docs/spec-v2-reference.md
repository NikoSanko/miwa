# 仕様乖離検知ツール（仮称） 要件・仕様 v2

> v1 からの主な変更: 保証範囲の明確化 / doc に不変 id を導入 / リンクはパス・ロックは id / 削除・復活・参照切れを含む状態モデル / ロックに合意時本文スナップショットを保持 / finding 判定表の全組合せ定義 / Judge は既定で助言のみ・`--auto-reconcile` 明示時のみ Jev 高確信を自動 reconcile / `--diff` の定義 / 終了コード / Go アダプターの受入条件

---

## 1. 何がしたいか

| ID | 目的 |
|---|---|
| G1 | 非開発メンバーが、業務要件と「最後にレビューされた状態から未合意の変更がないか」を Web 上で参照できる |
| G2 | 開発者・レビュアー・AI エージェントは、PR のドキュメント差分だけで振る舞いの変更を把握できる |

そのために、業務要件（doc）・技術要件（req）・テスト（test）の3層を依存関係で結び、どれかが更新されたのに残りが追随していない状態を **機械的に検知** し、それが本当に矛盾かどうかを **Judge で判定** する。

### 1.1 保証範囲

本ツールが保証するのは次の1点のみ:

> 登録された doc・req・test の関係について、現在の内容が **最後に合意（reconcile）した状態から変更されているか** を決定的に検出する。

保証しないもの（限界として明記する）:
- 要件の完全性（doc の要求が req 群で網羅されているか）、テストの十分性（req の全条件を test が検証しているか）
- 実装がテスト通りに動くこと（テスト成功は「実行した入力・assertion の範囲で失敗が観測されなかった」ことしか示さない）
- **実装コードだけを変更し doc / req / test を変えない PR**（D4 により検知対象外）
- テスト関数外の変更（ヘルパー・パッケージ変数のテーブル・fixture・golden file・`TestMain` 等）

前提となる考え方:
- 実装コードは直接扱わない。**テストを実装側の仕様とみなす**。実装とテストの一致はテストランナー（既存 CI）の責務とし、本ツールは「ドキュメント ↔ テスト」の追随だけを見る
- 機械的検知（ハッシュ比較）は決定的で安価なので CI とエージェントのゲートに使う。意味判定は確率的なので CI には入れず、ローカルで実行する
- テスト CI の成功、`check` の成功、意味レビューの承認は **それぞれ別の証拠** として扱う

### 1.2 G2 の成立条件

- req を変更すると、その req に依存する全 doc–req エッジと全 test–req エッジが未合意になる。gate を通すには、各エッジについて人手の合意、または明示的に有効化した Judge による合意（§9）が必要である
- 「doc 側まで伝播を強制する」とは、**doc 本文の変更ではなく doc–req エッジの再合意を要求する** という意味である。doc を直すか、「doc はそのままで良い」と合意するかはレビューの判断に委ねる
- test だけを変更し req を変えない場合、test–req エッジだけがドリフトする。ここで「req の範囲内の変更（リファクタ・ケース追加など）」として合意するのは、**req が変わらない限り仕様レベルの振る舞いは変わっていない** という定義に基づく判断である。判断の誤りはツールでは検知できず、PR レビューで担保する（合意の記録はロック差分として PR に現れる）
- 自動 reconcile による合意には、判定記録（`assessment`）の保存を必須とする。Jev は文章の理由を返さないため、判定記録はスコア・閾値・入力 digest などの機械的な記録であり、文章による説明は含まない（§9・§10）

## 2. 用語

| 用語 | 定義 |
|---|---|
| doc | 非開発メンバー向けの業務要件。OKF コンセプト（`type: Specification`） |
| req | 技術要件。doc に含まれる業務上の要求を具体化した、検証可能な技術上の要求。機能・非機能を含む。OKF コンセプト（`type: Requirement`） |
| test | req を検証するテスト関数 |
| エッジ | doc → req の参照1本、または test → req の参照1本 |
| 合意 | エッジ両端のその時点の内容を「整合している」とロックに記録した状態 |
| ドリフト | エッジの片側または両側が合意時から変更されている状態 |
| 孤立 | 依存元または依存先を持たないノード |
| 参照切れ | 参照は存在するが、参照先を解決できない状態 |
| テスト未紐付け | test が1件も紐づいていない req（v1 の「未カバー」を改名。紐づいていても十分性は保証しないため） |
| reconcile | 指定したエッジの現在の両端を「合意済み」としてロックに記録する操作 |
| retire | 消えたエッジの削除を承認し、ロックに削除承認記録（tombstone）を残す操作 |
| Judge | ドリフトが意味的に矛盾しているかを判定する、差し替え可能な判定機 |
| OKF | Open Knowledge Format。YAML フロントマター付き Markdown（採用版は §16 Q2 で確定） |

## 3. 参考資料

- OKF: [紹介記事](https://cloud.google.com/blog/ja/products/data-analytics/how-the-open-knowledge-format-can-improve-data-sharing/) / [仕様（旧所在）](https://github.com/GoogleCloudPlatform/knowledge-catalog/tree/main/okf) / [仕様（移転先・要確認）](https://github.com/GoogleCloudPlatform/open-knowledge-format/blob/main/SPEC.md)
- artgraph: [リポジトリ](https://github.com/mori-shin-x/artgraph) / [解説](https://zenn.dev/mrmtsntr/articles/artgraph-spec-code-drift)
- Drift: [リポジトリ](https://github.com/sho-ritz/drift) / [解説](https://zenn.dev/arupaka_sho/articles/drift_spec_code_sync_tool)
- jev-lint: [リポジトリ](https://github.com/mizchi/jev-lint) / [解説](https://zenn.dev/mizchi/articles/jev-lint-intro) / [Jev Library](https://jevlibrary.dev/projects/mizchi-jev-lint)
- Jev (TypeSafe AI): [公式](https://typesafe.ai) / [コミュニティまとめ](https://github.com/AnotiaWang/awesome-jev/wiki)
- tree-sitter: [本体](https://tree-sitter.github.io/tree-sitter/)

## 4. 参考ツールとの位置づけ

| ツール | 取り込む点 | 取らない点 |
|---|---|---|
| artgraph | 要求 ID 主キー、ロックによる決定的検知、`check --gate --diff` | コード層（`@impl`）、JS/TS 専用 |
| Drift | Fresh / Stale / Broken の状態モデル、検知と意味判定の分離 | ドキュメント ↔ 実装コードの直接リンク |
| jev-lint | 構造抽出 ＋ 自然言語ルール ＋ Jev バッチ評価、eval による閾値校正、キャッシュ | — |

本ツールの独自性:
1. テストを実装側の仕様として扱い、**テスト本文もハッシュ対象**にする（artgraph はテストをファイル参照のみ）
2. 意味判定を **Jev 一次 / LLM 二次** の2段構成にし、明示時のみ高確信の一致を自動 reconcile
3. 言語追加の単位を「tree-sitter 文法 ＋ 抽出クエリ ＋ 識別規則 ＋ 検証用サンプル」に定型化

## 5. スコープ

**v0**: 3層のグラフ構築、ハッシュ比較による検知（`scan` / `check`）、ロック更新（`reconcile` / `retire`）、ドリフトの意味判定（`judge`）。対応言語は Go。doc / req / test が同居する単一リポジトリ。

**v0 の G1**: GitHub 上で doc / req をそのまま閲覧する。検証状態の Web 表示は次スコープ（v0 では G1 は部分達成）。

## 6. モデル

```
doc ──依存──▶ req ◀──検証── test
 (n)          (1)          (n)
```

- req は誰にも依存しない。doc と test が req に依存する
- 1つの req は複数の doc から依存されてよい。1つの doc は複数の req に依存する
- 1つの req は複数の test で検証される。**1つの test が宣言できる req はちょうど1つ**（test → req は多対一）。マーカーが複数あれば構造エラー
- 1つの doc 内に同じ req へのリンクが複数あっても、エッジは1本とする（出現箇所は診断情報）

| 層 | 実体 | 識別子（不変キー） | 所在情報 | ハッシュ範囲 |
|---|---|---|---|---|
| doc | 業務要件ファイル | フロントマター `id`（例 `DOC-AUTH`） | ファイルパス | 本文全体（フロントマター除く） |
| req | 技術要件ファイル | フロントマター `id`（例 `FR-001`） | ファイルパス | 本文全体（フロントマター除く） |
| test | テスト関数 | パッケージディレクトリ ＋ 関数名 | ファイルパス・行 | tree-sitter で切り出した関数ノード |

- doc / req はファイルを移動しても識別子は変わらない。ただし doc 内のリンクの更新は必要（リンクが切れれば参照切れ）
- test は関数のリネーム・パッケージ移動で識別子が変わる。v0 では「旧エッジの削除 ＋ 新エッジの新規」として扱い、`check` は両者を同じレポートに並べて表示する（自動移行はしない）
- エッジ ID は `(種別, from ID, to ID)` から決定的に生成する（例 `doc-req:DOC-AUTH:FR-001`、`test-req:internal/auth:TestRejectsShortPassword:FR-001`）。ID の許容文字は `[A-Za-z0-9._/-]`、区切りは `:`

**ハッシュ**
- アルゴリズムは SHA-256
- 正規化は改行の LF 統一と行末空白の除去のみ。整形はリポジトリのフォーマッタに委ねる
- フロントマターは内容ハッシュに含めない。ただし `type` / `id` などグラフ・判定に影響するフィールドは **構造入力** として毎回検証する
- 正規化規則・抽出器には版番号を付け、ロックに記録する（版が変わったら §10 の移行扱い）

## 7. ファイル形式

**req**
```markdown
---
type: Requirement
id: FR-001
---
8文字未満のパスワードは登録時に拒否する。
```
- `id` は不変。ファイルパスは可読な任意パスでよい
- test の紐付けを免除する仕組みは持たない。Go テストで検証できない req も `no-linked-test` として報告される（免除の要否は運用で実測してから判断する。§15）

**doc**
```markdown
---
type: Specification
id: DOC-AUTH
---
# ログイン
ユーザーはメールアドレスとパスワードで認証する。
パスワードは [最小長の制約](../req/auth/password-min-length.md) に従う。
```
- **req 参照候補**: 本文中の Markdown リンク（インライン形式・参照形式）のうち、文書相対パスを解決した結果が **設定の req 探索領域の中にある** もの。リンク先の存在は問わない（解決前に候補かどうかが決まる）
- 候補は req の `id` に解決して doc → req のエッジにする。グラフとロックは `id` で扱う
- 候補のリンク先が存在しない、または `type: Requirement` でない場合は `broken-reference`（キーは「参照元 doc の id ＋ リンク先パス」）
- 候補でないリンク（req 探索領域の外への相対リンク、画像 `![]()`、外部 URL、絶対パス、アンカーのみのリンク）はエッジとして扱わず、検査もしない
- リンクのフラグメント（`#...`）は除去してから解決する
- 業務上の品質目標（非機能要件）も doc に書き、その測定条件と合格基準を req、検証処理を test に書く

**test（Go）**
```go
// [FR-001]
func TestRejectsShortPassword(t *testing.T) { ... }
```

Go アダプター v0 の受入条件:
- 対象は `*_test.go` のトップレベル関数 `func TestXxx(t *testing.T)`。Benchmark / Fuzz / Example は v0 では対象外
- マーカー `[<req id>]` は、関数の直前に隣接するコメントグループ（空行を挟まない）から取得する
- マーカーのない Test 関数はグラフに含めない（追跡対象外）。設定 `tests.require_marker: true` のときのみ「孤立 test」として報告する
- 関数内の `t.Run` サブテストは親 test の一部とする。サブテストごとに異なる req を検証したい場合はトップレベル関数を分ける
- build tags は無視して全ファイルを解析する。構文解析に失敗したファイルがあれば実行エラー（exit 1）
- 同じディレクトリに同名の Test 関数が複数ある場合（build tags で出し分けたファイル、`package x` と `package x_test` の併存など）は `duplicate-test-id`。どちらかを選んだり本文を連結したりはしない

## 8. 検出則（機械層）

`check` は毎回ソースから現在のグラフを構築し、ロックの合意記録と比較する。ロックは「現在状態の正解」ではなく「合意の記録」である。

### 8.1 構造 finding

| finding | 条件 |
|---|---|
| `duplicate-id` | 同じ `id` を持つ doc / req が複数ある |
| `duplicate-test-id` | 同じ test 識別子（パッケージディレクトリ ＋ 関数名）の関数が複数ある |
| `multiple-markers` | 1つの test に req マーカーが複数ある |
| `invalid-frontmatter` | `type` / `id` の欠落・不正 |
| `broken-reference` | doc の req 参照候補のリンク先を解決できない、または test のマーカーの req が存在しない |
| `lock-invalid` | ロックは JSON として読めるが、スキーマ違反、合意ハッシュの片方欠落、スナップショット欠落、スナップショットとハッシュの不整合がある |

- 構造 finding があるノードは、Judge・reconcile・retire の対象にしない
- `broken-reference` を持つノードに接続するエッジは retire できない（リンク切れを「意図的な削除」として承認させないため）
- `lock-invalid` があるときは、部分的に読めたエッジも含めてロックを Judge・更新の対象にしない

### 8.2 グラフ finding

| finding | 条件 |
|---|---|
| `orphan-req` | どの doc からも依存されない req |
| `orphan-doc` | どの req にも依存しない doc |
| `no-linked-test` | test が1件も紐づかない req |
| `orphan-test` | マーカーのない Test 関数（`tests.require_marker: true` のときのみ） |

### 8.3 エッジ finding（判定表）

現在のエッジ状態: `present`（参照があり両端を解決できる）/ `broken`（参照はあるが解決できない）/ `absent`（参照がない）。
`broken` を `absent` と同一視しない（リンク切れを「意図的な削除」として承認できてしまうため）。

既存エッジと参照切れの対応づけ:
- test–req: マーカーが req id を直接持つので、エッジ ID が決まる。そのまま `broken` として判定する
- doc–req: リンク先が解決できないと req id が分からない。そこでロックの各エッジに、合意時のリンク先パスを診断情報 `from_refs` として保存しておく。参照元 doc が一致し、`from_refs` にリンク先パスを含むエッジを `broken` とみなす
- 一致するエッジがない場合（リンクの書き換えと削除が同時に起きた場合など）は、`broken-reference` と、ロック上のエッジの `removed-unreviewed` の両方を報告する。この場合も、上記のとおり retire はできない

| ロック状態 | 現在のエッジ | from ハッシュ | to ハッシュ | finding |
|---|---|---|---|---|
| 記録なし | absent | — | — | なし |
| 記録なし | broken | — | — | `broken-reference` |
| 記録なし | present | — | — | `new-unreviewed` |
| active | absent | — | — | `removed-unreviewed` |
| active | broken | — | — | `broken-reference` |
| active | present | 合意値と同じ | 合意値と同じ | なし（fresh） |
| active | present | 変化 | 合意値と同じ | `changed-from` |
| active | present | 合意値と同じ | 変化 | `changed-to` |
| active | present | 変化 | 変化 | `changed-both` |
| retired | absent | — | — | なし |
| retired | broken | — | — | `broken-reference` |
| retired | present | — | — | `restored-unreviewed` |
| needs-review | absent | — | — | `removed-unreviewed` |
| needs-review | broken | — | — | `broken-reference` |
| needs-review | present | — | — | `migration-unreviewed` |

`changed-*` の読み方:
- doc–req `changed-from`（doc 変更・req 不変）→ req が追随していない疑い
- doc–req `changed-to`（req 変更・doc 不変）→ doc が追随していない疑い（依存元 doc ごとに独立して出る）
- test–req `changed-to`（req 変更・test 不変）→ test が追随していない疑い
- test–req `changed-from`（test 変更・req 不変）→ test が req の範囲内に収まっているかの確認が必要

状態遷移:
- `new-unreviewed` / `changed-*` / `restored-unreviewed` / `migration-unreviewed` → `reconcile` → active・fresh
- `removed-unreviewed` → `retire` → retired
- `broken-reference` → ソースの修正のみで解消する（reconcile / retire では消せない）
- 変更を完全に元に戻した場合は、再承認なしで fresh に戻る
- retired のエッジを reconcile して active に戻すとき、`retirement` は `last_retirement` に移して残す

### 8.4 `--diff`

- `B = merge-base(<base>, HEAD)`、`H = HEAD`。`<base>` は `--base <ref>` で指定し、省略時は設定の `diff.base`（既定 `origin/main`）
- B と H は、それぞれのツリーの **設定・ソース・ロック** で評価する。`--base` などの実行時引数は両方に共通で使う
- `--diff` はコミット同士の比較専用とし、H はコミット済みの HEAD とする。作業ツリーに未コミットの変更があれば、「差分検査の対象外」と警告する
- B と H のそれぞれで finding を算出し、**H にあって B にない finding のみ** を報告する
- finding の同一性キーは `(finding 種別, 対象 ID, evidence digest)`。evidence は finding ごとに正規化した内容とし、日時・理由文・表示位置は含めない

  | finding | evidence |
  |---|---|
  | `changed-*` / `new-unreviewed` / `restored-unreviewed` / `migration-unreviewed` | 両端の現在ハッシュと合意ハッシュ |
  | `removed-unreviewed` / `lock-entry-dropped` | ロック上の合意ハッシュ |
  | `broken-reference` | 参照元 ID と未解決の参照先（パスまたは id） |
  | `duplicate-id` / `duplicate-test-id` | 重複しているファイルパスの集合 |
  | `multiple-markers` | マーカーの集合 |
  | `invalid-frontmatter` / `lock-invalid` | 違反項目の集合 |
  | `orphan-*` / `no-linked-test` | 対象ノードの現在ハッシュ |

  同じエッジが B 時点から stale でも、さらに変更されていれば新しい finding になる
- B のロックが `lock-invalid` で比較できない場合は、集合差を取らずに実行エラー（exit 1）
- `--diff` のときだけ、ロック自体の変更を追加で検査する: B で active だったエントリが H のロックから消えていれば `lock-entry-dropped`（正規の削除は retire を経る）
- base が解決できない、履歴が足りない（shallow clone）、B と H で正規化規則・抽出器の版が違う場合は、成功とせず実行エラー（exit 1）。profile を移行する PR（§10）では `--diff` を使えないため、全件の `check --gate` で検査する
- CI で全件（`check --gate`）と差分（`check --gate --diff`）のどちらを使うかはプロジェクトに委ねる

### 8.5 gate と終了コード

`--gate` のとき、8.1〜8.4 の finding はすべて blocking とする（v0 では severity 設定は設けない）。

| exit | 意味 |
|---|---|
| 0 | 検査が完了した（`--gate` なしでは finding の有無を問わない。`--gate` ありでは blocking finding なし） |
| 1 | 実行エラー（I/O、設定不正、構文解析不能、base 解決不能、ロックが JSON として解析できないなど）で検査が完了しなかった |
| 2 | 検査が完了し、`--gate` で blocking finding を検出した |

ロックが JSON として読めない場合は exit 1。JSON として読めるが内容が不正な場合は `lock-invalid` という finding であり、他の finding と同じく `--gate` なしでは exit 0、`--gate` ありでは exit 2 とする。

通常の `check` / `scan` は作業ツリーを対象とする。未追跡ファイルも対象に含め、`.gitignore` と設定の除外パターンに該当するものは除く。

## 9. 意味判定（Judge）

**対象**: `changed-from` / `changed-to` / `changed-both` / `new-unreviewed` / `restored-unreviewed` / `migration-unreviewed` のエッジ。復活したエッジは、旧合意時の本文を参考情報として渡し、現在の関係を新たに評価する。構造 finding を持つノードのエッジは対象外。削除（`removed-unreviewed`）は整合性判定ができないため人手でのみ扱う。ローカルでのみ実行し、CI では実行しない。

**入力**: エッジ種別、変更された側、両端ノードの合意時本文（ロックのスナップショット。新規エッジにはない）と現在本文。
**出力**: `consistent | contradiction | uncertain`、Jev のスコア（主質問・補助質問）、判定に使った入力の digest、文章の根拠（二次の LLM を経由した場合のみ）。

Jev は型付きの値と確率だけを返し、文章を生成しない（Noul 型の戻り値は 0〜1 の数値 `noul` のみ）。そのため、Jev だけで consistent と判定された経路（自動 reconcile を含む）には文章の根拠が付かない。この経路で理由の文章を得るためだけに LLM を呼ぶことはしない（費用と速度の利点を損ない、しかも得られる文章は判定の後付けの説明にすぎないため）。

**質問（1エッジにつき、主質問1問 ＋ 除外条件の補助質問1問）**
- doc–req: 「req が扱う事項について、req は doc の記述と矛盾せず、doc の該当部分を具体化しているか」。doc 内で req が担当する範囲を特定できない場合は uncertain とする
  - 「矛盾しない」だけでは consistent としない（無関係な文章同士も矛盾しないため）
  - doc 全体が req 群で網羅されているかはエッジ単位では判定できないため、Judge の対象外（人手レビューの責務）
- test–req: 「test は req に記述された振る舞いの少なくとも一部を検証しており、req と矛盾する期待値を持たないか」。関数本文だけで判断できない場合（ヘルパー依存など）は uncertain とする

**一次: Jev**。Noul 型の質問（「この文は真か？」）を投げ、0〜1 のスコア `noul` を得る（校正の前提は §16 Q1 で要確認）。
質問は「関連していて、かつ矛盾しない」という複合条件なので、false に近いスコアの理由は「矛盾」「無関係」「判断材料不足」のどれかであり、スコアだけでは区別できない。そのため一次は **consistent と認めるかどうか** だけを決める。
**二次: LLM**。Jev が consistent と認めなかったとき、`contradiction | uncertain` を区別し、根拠を生成する。LLM の自己申告の確信度を Jev のスコアと同じ尺度で扱わない。

**ルーティング**（閾値は設定値。既定は仮置きで、eval による校正は次スコープ）

| Jev スコア p | 一次の結果 | 既定（助言モード） | `--auto-reconcile` 指定時 |
|---|---|---|---|
| p ≥ 0.9 かつ除外条件に該当しない | consistent | 表示のみ | 自動 reconcile（`by: jev`） |
| p ≥ 0.9 かつ除外条件に該当する | consistent と認めない | LLM が判定・根拠を表示 | 同左（reconcile しない） |
| p < 0.9 | consistent と認めない | LLM が `contradiction / uncertain` と根拠を表示 | 同左（reconcile しない） |

- 二次の LLM の出力は `contradiction | uncertain` のどちらかに限る（consistent は一次でしか出ない）
- 自動 reconcile の除外条件は「doc 内で req が担当する範囲を特定できない」「test の関数本文だけでは判断できない」の2つ。Jev への補助質問（「判断に必要な情報は入力に含まれているか」を Noul 型でもう1回問う）で得る。補助質問のスコアが閾値（設定値。既定は仮置きで 0.9）以上のときだけ「除外条件に該当しない」とする。補助質問が失敗した場合や取得できない場合は「該当する」とみなす。主質問・補助質問の両方のスコア、適用した閾値、質問の版を `assessment` に保存する
- v0 で自動 reconcile するのは、**`--auto-reconcile` を明示し、かつ上表の1行目に該当する** 場合に限る。それ以外はすべて人手の reconcile を待つ
- 自動 reconcile は、全対象の判定結果を集めてから、排他ロックの下で適用できる対象を再検証し、**1回の書き込み** でまとめて適用する（エッジごとに書き込むと、自分の書き込みで generation が変わって残りが失敗するため）。再検証を通らなかった対象だけを外す
- `judge` の終了時に、適用件数・未適用件数・失敗した対象を表示する
- 適用前の再検証（人手の reconcile / retire にも適用する）: ロックへの書き込み直前に現在のグラフを再構築し、判定開始時と次がすべて一致することを確認する。1つでも違えば適用しない
  - エッジ ID と両端の現在ハッシュ
  - 構造 finding の有無
  - profile と有効な設定の digest
  - ロックの世代（§10 の `generation`）
- 拒否・タイムアウト・不正な出力はロックを更新しない（uncertain として表示）
- doc 本文・test ソースは **データ** として Judge に渡し、Judge にファイル更新・コマンド実行の権限を与えない。出力はスキーマ検証する
- 外部送信は、プロジェクトの方針と実行者の許可を分けて管理する
  - プロジェクト方針（コミットする設定ファイル）: 使ってよい provider / endpoint の一覧
  - 実行者の許可（ユーザーローカル。例 `~/.config/specdrift/consent.json`）: リポジトリ、provider / endpoint、送信するデータの範囲（現在本文・合意時本文・test ソース）の組ごとに記録する
  - 初回の実行時に、一次（Jev）と二次（LLM）の両方の送信先と送信範囲を表示して許可を求める。送信先や範囲が増えたら、もう一度確認する
  - 許可がない provider には送信しない（二次への自動フォールバックもしない）

## 10. ロック

単一 JSON ファイル（既定 `.specdrift/lock.json`）。概念は「エッジごとの合意」「合意時本文のスナップショット」「削除承認」の3つ。

```json
{
  "schema_version": 1,
  "generation": 42,
  "profile": {
    "hash_algorithm": "sha256",
    "normalize": "specdrift-content-v1",
    "extractors": { "go": "go-test-v1" }
  },
  "snapshots": {
    "sha256:aaa…": { "text": "# ログイン\nユーザーは…\n" },
    "sha256:bbb…": { "text": "8文字未満のパスワードは登録時に拒否する。\n" },
    "sha256:ccc…": { "text": "func TestRejectsShortPassword(t *testing.T) {…}\n" }
  },
  "edges": {
    "doc-req:DOC-AUTH:FR-001": {
      "state": "active",
      "reconciled": { "from": "sha256:aaa…", "to": "sha256:bbb…" },
      "from_refs": ["req/auth/password-min-length.md"],
      "approval": {
        "by": "human",
        "reason": "最小長の記述が一致することを確認",
        "at": "2026-09-28T00:00:00Z"
      },
      "assessment": null
    },
    "test-req:internal/auth:TestRejectsShortPassword:FR-001": {
      "state": "active",
      "reconciled": { "from": "sha256:ccc…", "to": "sha256:bbb…" },
      "approval": { "by": "jev", "reason": null, "at": "2026-09-28T00:00:00Z" },
      "assessment": {
        "provider": "jev", "model": "…", "prompt_version": "1",
        "input_digest": "sha256:…", "verdict": "consistent",
        "scores": { "main": 0.94, "sufficiency": 0.92 },
        "thresholds": { "main": 0.9, "sufficiency": 0.9 },
        "rationale": null
      }
    }
  }
}
```

- スナップショットは正規化後の本文だけをハッシュのキーにし、`kind` は持たない（同じ本文の doc と req が同じキーを共有できるように。種別はエッジの端点から決まる）
- `generation` はロックを書き込むたびに1増やす。書き込みはロックファイルの排他ロック（例 `.specdrift/lock.json.lock`）を取ったうえで世代を比較し、判定開始時から変わっていれば中止する
- `from_refs` は doc–req エッジの合意時のリンク先パスの集合。リポジトリ相対に正規化し、整列して保存する。同一性や合意の判定には使わず、§8.3 の参照切れの対応づけにだけ使う
- 同じ doc から同じ req への参照のうち、一部が有効で一部が切れている場合、エッジは `present` とし、切れた参照は独立した `broken-reference` として報告する
- `approval.by`: `human | jev`。Judge を参考に人が reconcile した場合は `human`（`assessment` に Judge 結果を残す）
- `assessment` は機械的な判定記録。`scores` / `thresholds` は Jev の主質問（`main`）と補助質問（`sufficiency`）の値。`rationale` は二次の LLM を経由した場合だけ文章が入り、Jev だけで判定した場合は `null`
- `approval.by` は自己申告の監査用メタデータであり、承認の実体は PR レビューにある
- 理由の必須条件: 片側だけ変化したエッジ（`changed-from` / `changed-to`）の人手 reconcile、`retire`、`--all`
- retire 時は `state: "retired"` に遷移し、`retirement: { by, reason, at }` を追加する。エントリは削除しない
- スナップショットは active・retired のエントリから参照されるものだけを保持し、参照されなくなったものは reconcile 時に掃除する（過去の履歴は Git に任せる）。スナップショットには本文が残るため、機密を含む文書を扱う場合は注意
- 書式: キーはエッジ ID 順、固定インデント、変更しないエントリは書き換えない（並行 PR のコンフリクトを最小化するため）
- 書き込みは一時ファイルからの置換で原子的に行う。`check` / `scan` はロックに書き込まない
- 同じエッジで競合した場合は日時の新しい方を採らず、マージ後の内容で再度 reconcile する
- `profile`（正規化規則・抽出器の版）の移行（`reconcile --migrate`）
  - 実行前の条件: active のエッジはすべて fresh、retired のエッジはすべて absent であること。満たさなければ拒否する
  - **自動で移行できるのは、保存済みスナップショットの本文から新しい規則でハッシュを再計算でき、かつノードの同一性とハッシュ範囲が変わらない場合に限る**（例: 正規化規則の変更）。active・retired の両方のエントリに適用する
  - ハッシュ範囲が変わる移行（例: 抽出器がテスト関数外まで含めるようになった）は、スナップショットからは再計算できない
    - active のエッジは `state: "needs-review"` に遷移する。`reconciled` は旧 profile のハッシュのまま残し、`reconciled_profile` に旧 profile を記録する。`check` はこれを `migration-unreviewed` として報告し、通常の reconcile で active に戻す
    - retired のエッジは削除承認を失わないよう、`reconciled_profile` に旧 profile を記録して retired のまま保持する（absent である限り比較しないので再計算は不要）
    - `needs-review` は §8.3 の判定表では「合意値なし」として扱い、`lock-invalid` にはしない
  - 通常の `check` / `judge` / `reconcile` / `retire` の実行時に、ツールの正規化規則・抽出器の版がロックの `profile` と一致しない場合は、移行を案内して実行エラー（exit 1）とする。異なる規則で作ったハッシュ同士は比較しない
  - 新しいツールが旧 profile の移行に対応していない場合は実行エラー
  - 移行する PR は B と H で profile が違うので `--diff` を使えない（§8.4）。全件の `check --gate` で検査する

## 11. コマンド（v0）

| コマンド | 役割 | 実行場所 |
|---|---|---|
| `init` | 設定ファイル・空のロックを生成 | ローカル |
| `scan` | グラフ構築、構造検証、サマリ（ノード数・エッジ数） | ローカル / CI |
| `check [--gate] [--diff [--base <ref>]] [--format text\|json]` | finding の列挙。exit は §8.5 | ローカル / CI |
| `judge [--edge <id>…] [--auto-reconcile]` | 対象エッジの意味判定 | ローカル |
| `reconcile --edge <id>… [--reason <text>]` | 指定エッジを合意済みにする | ローカル |
| `reconcile --all --reason <text>` | 未合意の全エッジを一括で合意（初期導入用） | ローカル |
| `reconcile --migrate` | profile 変更に伴うハッシュの再計算 | ローカル |
| `retire --edge <id>… --reason <text>` | 消えたエッジの削除を承認 | ローカル |

- 引数なしの `reconcile` はロックを更新せず、未合意エッジの一覧と使い方を表示する
- `reconcile` / `retire` は、書き込む前に指定したすべての対象を検証する。1つでも不正な対象があれば何も更新しない（全件成功か無変更か）
- 対象ノードが0件の場合、`scan` / `check` は警告を出す（設定ミスと初期状態を区別するため）

**設定ファイル**（`.specdrift/config.yaml`）: doc / req の探索パス、test の探索パス、除外パターン、`tests.require_marker`、`diff.base`、Judge の閾値・送信許可。設定の変更も PR でレビューする（探索範囲を狭めると finding を消せるため）。

## 12. CI と公開（v0）

- CI では `check --gate` または `check --gate --diff` を実行する（どちらを使うかはプロジェクトが選ぶ）。Judge 用の API キーは CI に置かない
- `--diff` を使う場合は、merge-base を解決できるだけの履歴を checkout する（`fetch-depth: 0` など）
- SoT はリポジトリ。v0 では doc / req は GitHub 上でそのまま閲覧する（OKF は素の Markdown）

## 13. 非機能

- Rust 単一バイナリ、外部ランタイム不要
- `scan` / `check` は同一入力に対して同一出力（決定的。出力順はエッジ ID・ノード ID 順）。Judge 以外はネットワーク不要
- パスは `/` 区切りのリポジトリ相対に正規化する。シンボリックリンクはたどらない

## 14. 決定事項

| # | 決定 |
|---|---|
| D1 | CI の責務は更新漏れの決定的検知のみ |
| D2 | 意味判定はローカル。Jev 一次、LLM 二次 |
| D3 | v0 の Judge は既定で助言のみ。`--auto-reconcile` を明示したときだけ Jev consistent（p ≥ 閾値）を自動 reconcile する。閾値は仮置きで、eval で校正する |
| D4 | 実装コードへのリンクは持たない。3層固定 |
| D5 | doc と req は別 OKF コンセプト。doc → req に依存し、req は doc を知らない |
| D6 | doc ファイル全体を1ノードとする（節単位には分けない。1つの doc を小さく自己完結させる運用で、ドリフトのノイズを抑える） |
| D7 | 依存されていない req、依存先のない doc は孤立。マーカーのない test は既定では追跡対象外 |
| D8 | 1つの req を複数の doc から依存してよい |
| D9 | doc / req の識別子はフロントマター `id`。パスは可読な任意パス。移動してもロックは引き継がれるが、リンクの更新は必要 |
| D10 | test のハッシュ範囲は tree-sitter の関数ノード。識別子はパッケージディレクトリ ＋ 関数名 |
| D11 | Rust 単一バイナリで新規実装 |
| D12 | 仕様文書は OKF 準拠（採用版は Q2） |
| D13 | ロックはエッジ単位。合意時本文のスナップショットを持つ |
| D14 | Judge の質問型は Noul。一次（Jev）は主質問と補助質問の閾値で consistent と認めるかだけを決め、それ以外は二次（LLM）が contradiction / uncertain を区別する |
| D15 | 1つの test が宣言できる req は1つ（test → req は多対一）。複数の req に跨るなら test か req の粒度を見直す |
| D16 | 保証範囲は「最後の合意から未合意の変更がないこと」の検出に限る（§1.1） |
| D17 | エッジの削除は `retire` で明示的に承認し、tombstone を残す |
| D18 | `--diff` は機能として提供し、CI での採否はプロジェクトに委ねる |

## 15. 次のスコープ以降

- AI エージェント統合: Stop Hook で `check --gate [--diff]`、bootstrap / verify Skill
- 公開サイト生成（`graph --export`）: doc / req ごとの検証状態の表示（G1 の完全達成）
- PR 用レポート: base → head のノード・エッジ・合意・削除の変更一覧と reconcile 理由を Markdown で出力し、CI の Job Summary / PR コメントに載せる
- `rename`: req・doc の移動とリンクの書き換え、test のリネーム追従
- `eval`: ラベル付きケースによる Judge の精度計測と閾値校正（自動 reconcile の既定有効化の前提）
- 対応言語の追加（Python / Rust / C# / Swift / Java）
- Go のサブテスト単位マーカー、Benchmark / Fuzz 対応
- テストのヘルパー依存を任意でハッシュに含めるオプション
- test の紐付けを免除する仕組み（Go テストで検証できない req 向け）。v0 の運用で `no-linked-test` がどれだけ恒常的に残るかを実測してから要否を判断する
- ロックのエッジ単位ファイル分割、または専用マージドライバ（コンフリクトの実測次第）

## 16. 未確定事項

| # | 論点 | 状態 |
|---|---|---|
| Q1 | Jev の `noul` スコアの校正の前提（どの分布で校正された確率か）と、本ツールの質問（doc–req・test–req の整合）に対して校正が成り立つか | 一部確認済み: 戻り値は 0〜1 の `noul` のみで文章は返さない。補助質問も Noul 型の追加呼び出しで実現できる（[TypeSafe Docs](https://docs.typesafe.ai/)）。校正の前提は要確認で、それまで閾値は仮置き |
| Q2 | OKF の採用版。旧所在は移転しており、移転先の現行仕様が v0.2 になっているとの指摘あり。v0.1 固定（コミット指定）か v0.2 に追従するか | 要確認 |

## 17. 受入ケース（v0 の最小テスト）

片側変更（4パターン）/ 両側変更 / 新規エッジ / エッジ削除と retire / retired の復活 / 参照切れ（リンク先の削除・誤った id のマーカー）/ マーカーの削除 / マーカーの複数付与 / id の重複 / doc・req の移動（ロックが引き継がれる）/ test のリネーム（削除＋新規）/ 変更の完全な差し戻し（fresh に戻る）/ ロック破損・スナップショット欠落 / `--diff` での既存 finding の除外と追加変更の検出 / `lock-entry-dropped` / base 不明時の exit 1 / Judge 実行中に変更された場合の自動 reconcile 拒否 / profile の版違い / 同じ本文を持つ doc と req（スナップショットの共有）/ req ファイルを削除したときの doc–req（`broken-reference` になり retire できない）/ build tags で出し分けた同名 test（`duplicate-test-id`）/ Judge 実行中のマーカー・リンク先の変更（適用しない）/ ロックの同時更新（世代の不一致で中止）/ profile を移行する PR / req 探索領域の外への通常リンク（検査しない）/ `reconcile` の複数対象のうち1件が不正（何も更新しない）
