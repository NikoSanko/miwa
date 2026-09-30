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
2. **二次判定（Anthropic Claude）**: [Jev のスコアが設定のしきい値（`judge.threshold`）以上なら、その時点で `consistent`（整合）と判定して終わります](../req/judge-threshold-decides-consistent.md)。しきい値に届かなかったものだけを Claude に渡します。Claude は、[`contradiction`（矛盾）か `uncertain`（判断できない）かを決め、理由の文章を添えます](../req/judge-llm-request.md)。使うモデルは設定の `judge.llm_model` で決まります。

判定結果は次の3種類です。

| 判定 | 意味 |
| --- | --- |
| `consistent` | 整合している（Jev のスコアがしきい値以上） |
| `contradiction` | 矛盾している（理由つき） |
| `uncertain` | 判断できない（理由つき） |

Claude の応答が拒否されたり、途中で切れたり、読み取れなかったりした場合は、[そのエッジを `uncertain` とし、処理は続けます](../req/judge-llm-response-interpretation.md)。また、[1つの依存関係の本文が大きすぎる場合（両端の合計が 200,000 バイトを超える場合）は、送信せずに `uncertain`（理由「本文が大きすぎる」）とします](../req/judge-oversize-body-uncertain.md)。

一方、[ネットワークやサービス側のエラー（HTTP エラー、タイムアウト）は、実行エラーとして止まります](../req/judge-http-error-is-execution-error.md)。Jev が使えないときに Claude だけで代わりに判定する、ということはしません。[接続は 10 秒、1回の呼び出し全体は 60 秒でタイムアウトし](../req/judge-http-timeouts-and-secret-redaction.md)、エラーメッセージに API キーは含まれません。

## 外部サービスに送られるもの

`judge` は、対象の doc・req・test の **本文を外部サービスに送信します**。一次判定では TypeSafe AI（`api.typesafe.ai`）に、二次判定では Anthropic（`api.anthropic.com`）に送られます。社外に出せない内容を含むリポジトリでは、使う前に必ず確認してください。[実行を始めるときに、その旨が1行表示されます](../req/judge-announces-external-transmission.md)。

API キーは設定ファイルには書かず、環境変数で渡します。

- `TYPESAFE_API_KEY`: TypeSafe AI（Jev）用
- `ANTHROPIC_API_KEY`: Anthropic（Claude）用

[どちらかが設定されていない場合は、何も送信せずにエラーで終了します](../req/judge-api-keys-required.md)。しきい値やモデルの設定は [miwa を使い始める](setup.md) を参照してください。

## 結果の見方

`--format text`（既定）では、1依存関係につき1行で、[判定・依存関係の識別子・Jev のスコアが依存関係の識別子順に表示されます](../req/judge-output-text.md)。理由がある場合は、同じ行の最後にタブ区切りで付きます。

```
contradiction doc-req:DOC-AUTH:FR-001 score=0.31	最小長が doc では 12 文字、req では 8 文字になっている
consistent test-req:internal/auth:TestRejectsShortPassword:FR-001 score=0.94
```

`--format json` は他のツールと連携しやすい形式で、[同じ内容を機械可読な JSON で出力します](../req/judge-output-json.md)。

[判定が最後まで完了すれば、`contradiction` があっても終了コードは 0](../req/judge-exit-codes-and-no-targets.md) です。`judge` は助言をするコマンドで、CI のゲートではないためです。実行できなかった場合（設定や API キーの問題、通信エラーなど）は終了コード 1 になります。

## 自動で合意する（`--auto-reconcile`）

既定では、[`judge` は結果を表示するだけで、ロックを書き換えません](../req/judge-no-lock-change-without-flag.md)。

`--auto-reconcile` を付けたときだけ、`consistent` と判定された依存関係を自動で合意します。[すべての判定が終わってから1回でロックに書き込み、途中でエラーになった場合は何も書きません](../req/judge-auto-reconcile-writes-consistent.md)。記録には合意者 `by: jev` と Jev のスコアが残り、人が合意した場合（`by: human`）と区別できます。`contradiction` と `uncertain` の依存関係は、自動では合意されません。人が内容を確認して、[`reconcile`](reconcile.md) で合意するか、ソースを直してください。

## `removed` は対象外

ロックにあるのにソースから消えた依存関係（`removed`）は、`judge` の対象ではありません。消えた理由は意味判定では分からないため、人が判断します。復元するか、`reconcile --edge` で削除を承認してください（詳しくは [合意を記録する](reconcile.md) を参照）。
