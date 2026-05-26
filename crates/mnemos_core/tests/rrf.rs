use mnemos_core::retrieval::rrf::{rrf_fuse, RankedId};

#[test]
fn fusion_of_disjoint_lists_unions_ids() {
    let bm25 = vec![
        RankedId {
            id: "a".into(),
            rank: 1,
        },
        RankedId {
            id: "b".into(),
            rank: 2,
        },
    ];
    let dense = vec![
        RankedId {
            id: "c".into(),
            rank: 1,
        },
        RankedId {
            id: "d".into(),
            rank: 2,
        },
    ];
    let fused = rrf_fuse(&[&bm25, &dense], 60);
    let ids: Vec<&str> = fused.iter().map(|f| f.id.as_str()).collect();
    assert_eq!(ids.len(), 4);
    // a and c are both rank-1 → equal score → either order is fine, but they must be first two
    assert!(ids.iter().take(2).any(|i| *i == "a"));
    assert!(ids.iter().take(2).any(|i| *i == "c"));
}

#[test]
fn fusion_rewards_appearing_in_multiple_lists() {
    let bm25 = vec![RankedId {
        id: "x".into(),
        rank: 5,
    }]; // only mid-rank in BM25
    let dense = vec![RankedId {
        id: "x".into(),
        rank: 5,
    }]; // only mid-rank in Dense
    let other = vec![RankedId {
        id: "y".into(),
        rank: 1,
    }]; // top in third list
    let fused = rrf_fuse(&[&bm25, &dense, &other], 60);
    // x appears in 2 lists at rank 5; y in 1 list at rank 1.
    // x score = 2 × 1/(60+5) ≈ 0.0307
    // y score = 1 × 1/(60+1) ≈ 0.0164
    assert_eq!(fused[0].id, "x");
    assert!(fused[0].score > fused[1].score);
}

#[test]
fn fusion_with_no_lists_is_empty() {
    let fused = rrf_fuse(&[], 60);
    assert!(fused.is_empty());
}

#[test]
fn k_smaller_means_top_ranks_dominate_more() {
    let list = vec![
        RankedId {
            id: "a".into(),
            rank: 1,
        },
        RankedId {
            id: "b".into(),
            rank: 2,
        },
    ];
    let fused_k1 = rrf_fuse(&[&list], 1);
    let fused_k60 = rrf_fuse(&[&list], 60);
    let ratio_k1 = fused_k1[0].score / fused_k1[1].score;
    let ratio_k60 = fused_k60[0].score / fused_k60[1].score;
    assert!(
        ratio_k1 > ratio_k60,
        "smaller k should produce a bigger gap between rank 1 and rank 2"
    );
}
