---
type: Requirement
id: FR-008
---
doc 本文中の req ファイルへの相対リンクは、インライン形式（`[text](path)`）・参照形式（`[text][ref]` と `[ref]: path`）のいずれで書かれていても、doc → req のエッジとして抽出される。リンク先パスは doc ファイルの位置からの相対パスとして解決する。
