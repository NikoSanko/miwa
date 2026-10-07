---
type: Requirement
id: FR-088
---
`desc` エッジについて `check` が `drift` を報告するときの理由コードは、次のとおりとする。
- `from`（要約）のハッシュだけが合意時から変化していれば `DescriptionChanged`
- `to`（本文）のハッシュだけが変化していれば `BodyChanged`
- 両方が変化していれば `BothChanged`

ロックにない `desc` エッジは `new`、条件を満たさなくなったロック上の `desc` エッジは `removed` として、他のエッジと同じ規則で報告する（FR-086）。
Doc の本文を編集すると、次が別々の finding として報告される。
- その Doc の `doc-req` エッジ（リンクしている req の本数分）それぞれの `DocChanged`
- `desc` エッジの `BodyChanged`

Req が索引の根の配下にある場合は、Req の本文を編集すると、次が報告される。
- その Req を `to` とする各エッジの `ReqChanged`
- その Req の `desc` エッジの `BodyChanged`

要約の変更だけでは、`doc-req` と `test-req` の finding は出ない（FR-087）。
索引配下のノードから `description` を消すと、次の 2 件が報告される。
- `index` 種別の `MissingDescription`（FR-078）
- `desc` エッジの `drift`（`DescriptionChanged`。ロックに記録された要約が空でなかった場合）
