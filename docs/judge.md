---
type: Specification
id: DOC-JUDGE
---

# 意味が合っているか判定する（`judge`）

`check` は内容が「変わったこと」を機械的に見つけますが、それが本当に問題（矛盾）なのかまでは分かりません。たとえば doc の言い回しを直しただけなら、req やテストはそのままで構いません。`miwa judge` は、`check` が見つけた `drift` と `new` について、両端が意味的に整合しているかを判定して助言するコマンドです。

```
miwa judge
miwa judge --edge doc-req:DOC-AUTH:FR-001
miwa judge --auto-reconcile
```

`judge` は **ローカルでだけ使う機能** です。API キーが必要で、外部サービスにアクセスするため、CI では使いません（CI では `check --gate` を使います）。

## 判定の対象

[対象は `check` が `drift` または `new` と判定する依存関係（doc と req、test と req のどちらも）](../req/judge-target-edges.md)です。`--edge` を付けると、指定した依存関係だけに絞れます。対象でない ID を指定するとエラーになります。リンク切れ（`broken`）や不正な状態（`invalid`）に関わる依存関係は、先にソースを直す必要があるため対象になりません。

対象が1件もない場合は、[「対象なし」と表示して正常終了します](../req/judge-exit-codes-and-no-targets.md)。

## 2段階で判定する

`judge` は、依存関係の両端の **現在の本文** を読んで、次の2段階で判定します。

1. **一次判定（Jev）**: TypeSafe AI の Jev が、「整合しているか」を 0〜1 のスコアで答えます。[doc と req の組、または test と req の組を1組ずつ Jev に送ります](../req/judge-jev-request.md)。1に近いほど整合しているという意味です。Jev が返すのは数値だけで、文章による理由は返しません。[スコアが取り出せない応答は、実行エラーとして扱います](../req/judge-jev-response-score.md)。
2. **二次判定（LLM エージェント）**: [Jev のスコアが設定のしきい値（`judge.threshold`）以上なら、その依存関係は `consistent`（整合）として扱い、そこで終わりです](../req/judge-threshold-decides-consistent-or-review.md)。しきい値に届かなかったものは `review`（見直しが必要）となり、LLM エージェントに任せます。

### エージェントに任せる

`review` の依存関係が1件でもあると、miwa は設定 `judge.llm_command` で指定された LLM エージェントの CLI（既定では Claude Code）を、[1回だけ起動します](../req/judge-agent-invocation.md)。まとめて渡すので、依存関係が多くても起動は1回です。エージェントの出力は miwa が加工せずそのまま画面に流します。

エージェントには、[依存関係ごとに、識別子・種別・両端のファイルの場所と本文・Jev のスコア](../req/judge-agent-prompt-edge-details.md)が渡されます。あわせて[次のような指示](../req/judge-agent-prompt-instructions.md)が渡されます。

- 各依存関係について、両端が意味的に整合しているかを判定し、結論と理由を報告する
- doc・req・test のファイルは編集しない

つまり、矛盾しているかどうかの判断と理由の説明は、エージェントが行います。miwa はその文章を解釈しません。エージェントが起動できなかった場合や、エラーで終了した場合は、[実行エラーとして止まります](../req/judge-exit-codes-and-no-targets.md)。

なお、[1つの依存関係の本文が大きすぎる場合（両端の合計が 200,000 バイトを超える場合）は、Jev にもエージェントにも渡さず、`skipped`（見送り）と表示します](../req/judge-oversize-body-skipped.md)。

## 外部サービスに送られるもの

`judge` は、対象の doc・req・test の **本文を外部に渡します**。一次判定では TypeSafe AI（`api.typesafe.ai`）に、二次判定では設定されたエージェント CLI（およびその先の LLM サービス）に渡ります。社外に出せない内容を含むリポジトリでは、使う前に必ず確認してください。[実行を始めるときに、その旨が1行表示されます](../req/judge-announces-external-transmission.md)。

TypeSafe AI の API キーは、設定ファイルには書かず、環境変数 `TYPESAFE_API_KEY` で渡します。[設定されていない場合は、何も送信せずにエラーで終了します](../req/judge-api-key-required.md)。エージェント CLI 側の認証は、そのツール自身の方法で行ってください。しきい値やエージェントのコマンドの設定は [miwa を使い始める](setup.md) を参照してください。

通信エラーやタイムアウトは、[一次判定の実行エラーとして止まり、エラーメッセージに API キーは含まれません](../req/judge-jev-http-errors-timeouts-and-redaction.md)。

## 結果の見方

miwa 自身は、[一次判定の結果を、1依存関係につき1行で、依存関係の識別子順に表示します](../req/judge-output-text.md)。

```
consistent test-req:internal/auth:TestRejectsShortPassword:FR-001 score=0.94
review doc-req:DOC-AUTH:FR-001 score=0.31
skipped doc-req:DOC-BIG:FR-090 score=-
```

- `consistent`: Jev が整合していると判断した
- `review`: しきい値に届かず、エージェントに委任した
- `skipped`: 本文が大きすぎて判定できなかった

この一覧の後に、エージェントの判定結果（結論と理由）が続きます。

[判定が最後まで完了すれば、`review` や `skipped` があっても終了コードは 0](../req/judge-exit-codes-and-no-targets.md) です。`judge` は助言をするコマンドで、CI のゲートではないためです。実行できなかった場合（設定や API キーの問題、通信エラー、エージェントの失敗など）は終了コード 1 になります。

## 自動で合意する（`--auto-reconcile`）

既定では、`judge` は表示と報告だけで、ロックを書き換えません。エージェントにも、合意（reconcile）は実行しないよう指示します。

`--auto-reconcile` を付けたときだけ、合意まで進みます。[ロックへの記録は2か所で行われます](../req/judge-auto-reconcile-lock-policy.md)。

- **Jev が `consistent` と判断したもの**: エージェントを起動する前に、miwa が1回でまとめてロックに記録します。途中でエラーになった場合は何も書きません。記録には合意者 `by: jev` と Jev のスコアが残ります。
- **エージェントが整合していると判断したもの**: エージェントが `miwa reconcile --edge <id> --reason "<理由>"` を実行して合意します。通常の `reconcile` と同じ仕組みなので、合意者は `by: human`、理由はエージェントの書いたものになります。矛盾していると判断したものは合意せず、どちら側を直すべきかが報告されます。

エージェントが自分で判断して合意まで行うため、`--auto-reconcile` を使うときは、報告された理由をあとで確認することをおすすめします。

## `removed` は対象外

ロックにあるのにソースから消えた依存関係（`removed`）は、`judge` の対象ではありません。消えた理由は意味判定では分からないため、人が判断します。復元するか、`reconcile --edge` で削除を承認してください（詳しくは [合意を記録する](reconcile.md) を参照）。
