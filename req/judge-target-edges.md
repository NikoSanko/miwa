---
type: Requirement
id: FR-038
---
`judge [--edge <id>]... [--auto-reconcile]` の判定対象は、`check` が `drift` または `new` と判定するエッジとする。エッジは、doc–req・test–req・desc の 3種である。次のエッジは対象にしない。`broken` または `invalid` の finding に関わるエッジ、`removed` のエッジ、finding のないエッジである。

`--edge` を 1つ以上指定した場合は、指定された ID のエッジだけを対象とする。同じ ID の重複指定は 1つとして扱う。指定された ID が対象外（存在しない、finding なし、`removed`、`broken` / `invalid` 関連を含む）であれば、API を呼ばずに exit 1 とする。

`judge` は `--format` オプションを持たない。
