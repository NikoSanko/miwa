---
type: Requirement
id: FR-046
---
エージェントに渡すプロンプトには、`review` の各エッジについて、エッジ ID、種別（doc–req / test–req）、両端のファイルパス、両端の現在の本文、Jev のスコアを含める。エッジはエッジ ID の昇順に並べる。`consistent` のエッジと、本文超過で `skipped` になったエッジは含めない。
