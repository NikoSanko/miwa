//! Block alignment and character-level diff used by `miwa site --diff`.

use std::ops::Range;

/// Minimum similarity for pairing a deleted block with an added block.
pub const SIMILARITY_THRESHOLD: f64 = 0.45;
/// Maximum number of equal blocks absorbed between changes.
pub const ABSORB_EQUAL_MAX: usize = 2;
/// Maximum combined chars for a character-level diff of a block pair.
pub const CHAR_DIFF_MAX_CHARS: usize = 4_000;
/// Maximum product of deleted and added block counts for pairing.
pub const PAIRING_MAX_PRODUCT: usize = 10_000;
/// Maximum number of blocks per document before falling back.
pub const DOC_MAX_BLOCKS: usize = 20_000;
/// Maximum edit distance accepted by Myers diff.
pub const MYERS_MAX_EDITS: usize = 2_000;

/// One edit-script operation over index ranges.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Op {
    Equal { a: Range<usize>, b: Range<usize> },
    Delete { a: Range<usize> },
    Insert { b: Range<usize> },
}

/// Myers diff. Returns None when the edit distance exceeds `max_edits`.
pub fn diff_ops<T: Eq>(_a: &[T], _b: &[T], _max_edits: usize) -> Option<Vec<Op>> {
    todo!()
}

/// Multiset character overlap: 2 * common / (len_a + len_b). Both empty -> 1.0.
pub fn similarity(_a: &str, _b: &str) -> f64 {
    todo!()
}

/// Kind of a rendered block.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BlockKind {
    Paragraph,
    Heading,
    Code,
    Table,
    ListItem,
    Quote,
    Rule,
    Footnote,
    Html,
    Other,
}

/// A block with its comparison key and similarity text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockKey {
    pub kind: BlockKind,
    /// Comparison key (unprefixed HTML + image hashes).
    pub key: String,
    /// Plain text used for similarity.
    pub text: String,
}

/// Row classification in the side-by-side view.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowKind {
    Same,
    Changed,
    Added,
    Deleted,
}

/// One aligned row; indices point into the left / right block sequences.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Row {
    pub kind: RowKind,
    pub left: Option<usize>,
    pub right: Option<usize>,
}

/// Aligns two block sequences per FR-117 (limits included).
pub fn align_blocks(_a: &[BlockKey], _b: &[BlockKey]) -> Vec<Row> {
    todo!()
}

/// Character-level marks per FR-117: `Some((changed_in_a, changed_in_b))` with one bool per char,
/// or None when the pair exceeds CHAR_DIFF_MAX_CHARS or MYERS_MAX_EDITS (whole block changed, no highlight).
pub fn char_marks(_a: &[char], _b: &[char]) -> Option<(Vec<bool>, Vec<bool>)> {
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rebuild<T: Clone + Eq + std::fmt::Debug>(a: &[T], b: &[T], ops: &[Op]) {
        let mut from_a = Vec::new();
        let mut from_b = Vec::new();
        for op in ops {
            match op {
                Op::Equal { a: ra, b: rb } => {
                    assert_eq!(a[ra.clone()], b[rb.clone()]);
                    from_a.extend_from_slice(&a[ra.clone()]);
                    from_b.extend_from_slice(&b[rb.clone()]);
                }
                Op::Delete { a: ra } => from_a.extend_from_slice(&a[ra.clone()]),
                Op::Insert { b: rb } => from_b.extend_from_slice(&b[rb.clone()]),
            }
        }
        assert_eq!(from_a, a);
        assert_eq!(from_b, b);
    }

    fn has_equal(ops: &[Op]) -> bool {
        ops.iter().any(|o| matches!(o, Op::Equal { .. }))
    }
    fn has_delete(ops: &[Op]) -> bool {
        ops.iter().any(|o| matches!(o, Op::Delete { .. }))
    }
    fn has_insert(ops: &[Op]) -> bool {
        ops.iter().any(|o| matches!(o, Op::Insert { .. }))
    }

    // ------------------------------------------------------------ diff_ops

    // [FR-117]
    #[test]
    fn diff_opsは両方空なら空の編集列を返す() {
        let ops = diff_ops::<i32>(&[], &[], 10).unwrap();
        assert!(!has_delete(&ops) && !has_insert(&ops));
        rebuild::<i32>(&[], &[], &ops);
    }

    // [FR-117]
    #[test]
    fn diff_opsは同一列ならequalだけを返す() {
        let a = [1, 2, 3];
        let ops = diff_ops(&a, &a, 10).unwrap();
        assert!(has_equal(&ops));
        assert!(!has_delete(&ops) && !has_insert(&ops));
        rebuild(&a, &a, &ops);
    }

    // [FR-117]
    #[test]
    fn diff_opsは全置換なら削除と挿入だけでequalを含まない() {
        let (a, b) = ([1, 2, 3], [4, 5, 6]);
        let ops = diff_ops(&a, &b, 100).unwrap();
        assert!(!has_equal(&ops));
        assert!(has_delete(&ops) && has_insert(&ops));
        rebuild(&a, &b, &ops);
    }

    // [FR-117]
    #[test]
    fn diff_opsは挿入のみなら削除を含まない() {
        let (a, b) = ([1, 3], [1, 2, 3]);
        let ops = diff_ops(&a, &b, 100).unwrap();
        assert!(has_insert(&ops) && !has_delete(&ops));
        rebuild(&a, &b, &ops);
    }

    // [FR-117]
    #[test]
    fn diff_opsは削除のみなら挿入を含まない() {
        let (a, b) = ([1, 2, 3], [1, 3]);
        let ops = diff_ops(&a, &b, 100).unwrap();
        assert!(has_delete(&ops) && !has_insert(&ops));
        rebuild(&a, &b, &ops);
    }

    // [FR-117]
    #[test]
    fn diff_opsは編集距離がmax_editsを超えるとnoneで等しければsome() {
        let (a, b) = ([1, 2, 3], [4, 5, 6]);
        assert!(diff_ops(&a, &b, 5).is_none());
        let ops = diff_ops(&a, &b, 6).expect("distance 6 is within max_edits 6");
        rebuild(&a, &b, &ops);
    }

    // [FR-117]
    #[test]
    fn diff_opsは混在した変更でも両列を復元できる() {
        let a: Vec<char> = "kitten sitting".chars().collect();
        let b: Vec<char> = "sitting kitten".chars().collect();
        let ops = diff_ops(&a, &b, MYERS_MAX_EDITS).unwrap();
        rebuild(&a, &b, &ops);
    }

    // ---------------------------------------------------------- similarity

    // [FR-117]
    #[test]
    fn similarityは両方空なら1() {
        assert_eq!(similarity("", ""), 1.0);
    }

    // [FR-117]
    #[test]
    fn similarityは片側だけ空なら0() {
        assert_eq!(similarity("", "abc"), 0.0);
        assert_eq!(similarity("abc", ""), 0.0);
    }

    // [FR-117]
    #[test]
    fn similarityは同一文字列なら1で共通が無ければ0() {
        assert_eq!(similarity("abc", "abc"), 1.0);
        assert_eq!(similarity("abc", "xyz"), 0.0);
    }

    // [FR-117]
    #[test]
    fn similarityは文字の多重集合の重なりで定まる() {
        // a: {a:2, b:1}, b: {a:1, b:2} -> common 2 -> 2*2/6
        let v = similarity("aab", "abb");
        assert!((v - 4.0 / 6.0).abs() < 1e-9, "{v}");
        // order does not matter
        assert_eq!(similarity("abcd", "dcba"), 1.0);
        let w = similarity("abcd", "abxy");
        assert!((w - 0.5).abs() < 1e-9, "{w}");
    }

    // -------------------------------------------------------- align_blocks

    fn bk(kind: BlockKind, key: &str, text: &str) -> BlockKey {
        BlockKey {
            kind,
            key: key.to_string(),
            text: text.to_string(),
        }
    }

    fn para(key: &str, text: &str) -> BlockKey {
        bk(BlockKind::Paragraph, key, text)
    }

    fn count(rows: &[Row], kind: RowKind) -> usize {
        rows.iter().filter(|r| r.kind == kind).count()
    }

    /// 20 distinct chars, optionally with the first `keep` shared and the rest replaced.
    fn text20(keep: usize, other: bool) -> String {
        let base: Vec<char> = ('a'..='t').collect();
        let alt: Vec<char> = ('A'..='T').collect();
        (0..20)
            .map(|i| if i < keep || !other { base[i] } else { alt[i] })
            .collect()
    }

    // [FR-117]
    #[test]
    fn align_blocksは同一列ならすべてsameで添字が揃う() {
        let a = vec![para("1", "x"), para("2", "y"), para("3", "z")];
        let rows = align_blocks(&a, &a.clone());
        assert_eq!(rows.len(), 3);
        for (i, r) in rows.iter().enumerate() {
            assert_eq!(r.kind, RowKind::Same);
            assert_eq!((r.left, r.right), (Some(i), Some(i)));
        }
    }

    // [FR-117]
    #[test]
    fn align_blocksは両方空なら行も空() {
        assert!(align_blocks(&[], &[]).is_empty());
    }

    // [FR-117]
    #[test]
    fn align_blocksは片側だけのブロックを追加と削除にする() {
        let a = vec![para("1", "x")];
        let rows = align_blocks(&a, &[]);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].kind, RowKind::Deleted);
        assert_eq!((rows[0].left, rows[0].right), (Some(0), None));
        let rows = align_blocks(&[], &a);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].kind, RowKind::Added);
        assert_eq!((rows[0].left, rows[0].right), (None, Some(0)));
    }

    // [FR-117]
    #[test]
    fn align_blocksは類似度が閾値以上の置換区間のブロックを対にする() {
        // 10 common of 20 -> 0.5
        let a = vec![para("l", &text20(10, false))];
        let b = vec![para("r", &text20(10, true))];
        let rows = align_blocks(&a, &b);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].kind, RowKind::Changed);
        assert_eq!((rows[0].left, rows[0].right), (Some(0), Some(0)));
    }

    // [FR-117]
    #[test]
    fn align_blocksは類似度がちょうど0_45なら対にする() {
        // 9 common of 20+20 -> 18/40 = 0.45
        let a = vec![para("l", &text20(9, false))];
        let b = vec![para("r", &text20(9, true))];
        assert!((similarity(&a[0].text, &b[0].text) - 0.45).abs() < 1e-9);
        let rows = align_blocks(&a, &b);
        assert_eq!(count(&rows, RowKind::Changed), 1);
    }

    // [FR-117]
    #[test]
    fn align_blocksは類似度が閾値未満なら対にせず削除と追加にする() {
        // 8 common of 20+20 -> 0.4
        let a = vec![para("l", &text20(8, false))];
        let b = vec![para("r", &text20(8, true))];
        let rows = align_blocks(&a, &b);
        assert_eq!(count(&rows, RowKind::Changed), 0);
        assert_eq!(count(&rows, RowKind::Deleted), 1);
        assert_eq!(count(&rows, RowKind::Added), 1);
    }

    // [FR-117]
    #[test]
    fn align_blocksは先読みして良い相手が後ろにあれば手前を単独の追加にする() {
        // R0 is similar enough (0.5) to L0, but R1 is identical in text.
        let l = vec![para("l", &text20(0, false))];
        let r = vec![
            para(
                "r0",
                &format!("{}{}", &text20(0, false)[..10], "XXXXXXXXXX"),
            ),
            para("r1", &text20(0, false)),
        ];
        let rows = align_blocks(&l, &r);
        assert_eq!(count(&rows, RowKind::Changed), 1);
        let changed = rows.iter().find(|r| r.kind == RowKind::Changed).unwrap();
        assert_eq!((changed.left, changed.right), (Some(0), Some(1)));
        let added = rows.iter().find(|r| r.kind == RowKind::Added).unwrap();
        assert_eq!(added.right, Some(0));
        assert_eq!(count(&rows, RowKind::Deleted), 0);
    }

    // [FR-117]
    #[test]
    fn align_blocksは対にしたブロックの順序を保つ() {
        let a = vec![para("l0", "alpha beta"), para("l1", "gamma delta")];
        let b = vec![para("r0", "alpha beta!"), para("r1", "gamma delta!")];
        let rows = align_blocks(&a, &b);
        let pairs: Vec<_> = rows
            .iter()
            .filter(|r| r.kind == RowKind::Changed)
            .map(|r| (r.left.unwrap(), r.right.unwrap()))
            .collect();
        assert_eq!(pairs, vec![(0, 0), (1, 1)]);
    }

    // [FR-117]
    #[test]
    fn align_blocksは空テキストで種類が同じブロックを対にする() {
        let a = vec![bk(BlockKind::Paragraph, "img-a", "")];
        let b = vec![bk(BlockKind::Paragraph, "img-b", "")];
        let rows = align_blocks(&a, &b);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].kind, RowKind::Changed);
    }

    // [FR-117]
    #[test]
    fn align_blocksは空テキストで種類が違うブロックを対にしない() {
        let a = vec![bk(BlockKind::Paragraph, "img-a", "")];
        let b = vec![bk(BlockKind::ListItem, "img-b", "")];
        let rows = align_blocks(&a, &b);
        assert_eq!(count(&rows, RowKind::Changed), 0);
        assert_eq!(count(&rows, RowKind::Deleted), 1);
        assert_eq!(count(&rows, RowKind::Added), 1);
    }

    fn many(prefix: &str, n: usize, same_text: bool) -> Vec<BlockKey> {
        (0..n)
            .map(|i| {
                let text = if same_text {
                    format!("text number {i}")
                } else {
                    format!("{prefix}{i}")
                };
                para(&format!("{prefix}{i}"), &text)
            })
            .collect()
    }

    // [FR-117]
    #[test]
    fn align_blocksは積が上限ちょうどなら対づけする() {
        let a = many("l", 100, true);
        let b = many("r", 100, true);
        assert_eq!(a.len() * b.len(), PAIRING_MAX_PRODUCT);
        let rows = align_blocks(&a, &b);
        assert_eq!(count(&rows, RowKind::Changed), 100);
    }

    // [FR-117]
    #[test]
    fn align_blocksは積が上限を超えると対づけせずすべて削除と追加にする() {
        let a = many("l", 101, true);
        let b = many("r", 100, true);
        assert!(a.len() * b.len() > PAIRING_MAX_PRODUCT);
        let rows = align_blocks(&a, &b);
        assert_eq!(count(&rows, RowKind::Changed), 0);
        assert_eq!(count(&rows, RowKind::Deleted), 101);
        assert_eq!(count(&rows, RowKind::Added), 100);
    }

    // [FR-117]
    #[test]
    fn align_blocksはブロック数の和が上限ちょうどなら通常どおり比べる() {
        let a = many("k", DOC_MAX_BLOCKS / 2, false);
        let rows = align_blocks(&a, &a.clone());
        assert_eq!(rows.len(), DOC_MAX_BLOCKS / 2);
        assert!(rows.iter().all(|r| r.kind == RowKind::Same));
    }

    // [FR-117]
    #[test]
    fn align_blocksはブロック数の和が上限を超えると全削除の後に全追加にする() {
        let a = many("k", DOC_MAX_BLOCKS / 2 + 1, false);
        let b = many("k", DOC_MAX_BLOCKS / 2, false);
        let rows = align_blocks(&a, &b);
        assert_eq!(rows.len(), a.len() + b.len());
        let (del, add) = rows.split_at(a.len());
        assert!(del
            .iter()
            .all(|r| r.kind == RowKind::Deleted && r.right.is_none()));
        assert!(add
            .iter()
            .all(|r| r.kind == RowKind::Added && r.left.is_none()));
        assert_eq!(del[0].left, Some(0));
        assert_eq!(add[0].right, Some(0));
    }

    // ---------------------------------------------------------- char_marks

    fn chars(s: &str) -> Vec<char> {
        s.chars().collect()
    }

    // [FR-117]
    #[test]
    fn char_marksは同一なら印を付けない() {
        let a = chars("同じ文字列");
        let (ma, mb) = char_marks(&a, &a.clone()).unwrap();
        assert_eq!(ma, vec![false; 5]);
        assert_eq!(mb, vec![false; 5]);
    }

    // [FR-117]
    #[test]
    fn char_marksは変わった文字だけに印を付ける() {
        let (ma, mb) = char_marks(&chars("abcdefgh"), &chars("abXdefgh")).unwrap();
        assert_eq!(
            ma,
            vec![false, false, true, false, false, false, false, false]
        );
        assert_eq!(
            mb,
            vec![false, false, true, false, false, false, false, false]
        );
    }

    // [FR-117]
    #[test]
    fn char_marksは2文字以下の一致に挟まれた変更を1つにまとめる() {
        let (ma, mb) = char_marks(&chars("AAxxBB"), &chars("CCxxDD")).unwrap();
        assert_eq!(ma, vec![true; 6]);
        assert_eq!(mb, vec![true; 6]);
    }

    // [FR-117]
    #[test]
    fn char_marksは1文字の一致に挟まれた変更もまとめる() {
        let (ma, mb) = char_marks(&chars("AAxBB"), &chars("CCxDD")).unwrap();
        assert_eq!(ma, vec![true; 5]);
        assert_eq!(mb, vec![true; 5]);
    }

    // [FR-117]
    #[test]
    fn char_marksは3文字の一致に挟まれた変更はまとめない() {
        let (ma, mb) = char_marks(&chars("AAxxxBB"), &chars("CCxxxDD")).unwrap();
        assert_eq!(ma, vec![true, true, false, false, false, true, true]);
        assert_eq!(mb, vec![true, true, false, false, false, true, true]);
    }

    // [FR-117]
    #[test]
    fn char_marksは挿入と削除を別々の側に印付けする() {
        let (ma, mb) = char_marks(&chars("abcdefgh"), &chars("abcdXXefgh")).unwrap();
        assert_eq!(ma.len(), 8);
        assert_eq!(mb.len(), 10);
        assert!(ma.iter().all(|m| !m));
        assert_eq!(mb.iter().filter(|m| **m).count(), 2);
        assert!(mb[4] && mb[5]);
    }

    // [FR-117]
    #[test]
    fn char_marksは文字数の和が上限ちょうどなら印を返す() {
        let a = vec!['a'; CHAR_DIFF_MAX_CHARS / 2];
        let b = vec!['a'; CHAR_DIFF_MAX_CHARS / 2];
        assert!(char_marks(&a, &b).is_some());
    }

    // [FR-117]
    #[test]
    fn char_marksは文字数の和が上限を超えるとnone() {
        let a = vec!['a'; CHAR_DIFF_MAX_CHARS / 2];
        let b = vec!['a'; CHAR_DIFF_MAX_CHARS / 2 + 1];
        assert!(char_marks(&a, &b).is_none());
    }

    // [FR-117]
    #[test]
    fn char_marksは編集距離が上限を超えるとnone() {
        // sum == 4000 (allowed), but edit distance 4000 > MYERS_MAX_EDITS
        let a = vec!['a'; CHAR_DIFF_MAX_CHARS / 2];
        let b = vec!['b'; CHAR_DIFF_MAX_CHARS / 2];
        assert!(char_marks(&a, &b).is_none());
    }

    // [FR-117]
    #[test]
    fn char_marksは編集距離が上限以内なら印を返す() {
        let a = vec!['a'; 500];
        let b = vec!['b'; 500];
        let (ma, mb) = char_marks(&a, &b).unwrap();
        assert!(ma.iter().all(|m| *m) && mb.iter().all(|m| *m));
    }
}
