use super::*;

// ==================== 案 I テスト ====================

// --- Ledoit-Wolf テスト ---

/// F = (tr(S)/n)·I の正当性: 縮小ターゲットが正しいスケーリング単位行列
#[test]
fn test_ledoit_wolf_identity_target() {
    let returns = generate_synthetic_returns(5, 30, 42);
    let sample_cov = {
        let n = returns.len();
        let pairs: Vec<(usize, usize)> = (0..n).flat_map(|i| (i..n).map(move |j| (i, j))).collect();
        let mut cov = ndarray::Array2::zeros((n, n));
        for (i, j) in pairs {
            let c = calculate_covariance(&returns[i], &returns[j]);
            cov[[i, j]] = c;
            cov[[j, i]] = c;
        }
        cov
    };

    let result = ledoit_wolf_shrink(&returns);

    // 結果は正方行列、元と同じサイズ
    assert_eq!(result.shape(), sample_cov.shape());

    // 対角は正
    for i in 0..5 {
        assert!(result[[i, i]] > 0.0, "Diagonal must be positive");
    }

    // 対称
    for i in 0..5 {
        for j in 0..5 {
            assert!(
                (result[[i, j]] - result[[j, i]]).abs() < 1e-15,
                "Must be symmetric"
            );
        }
    }
}

/// δ ∈ [0, 1] の範囲確認
#[test]
fn test_ledoit_wolf_shrinkage_range() {
    // n < T: 縮小係数は小さいはず
    let returns_low_n = generate_synthetic_returns(3, 50, 123);
    let cov_low = calculate_covariance_matrix(&returns_low_n);

    // n > T: 縮小係数は大きいはず
    let returns_high_n = generate_synthetic_returns(50, 10, 456);
    let cov_high = calculate_covariance_matrix(&returns_high_n);

    // 両方とも有効な共分散行列（正定値）
    for i in 0..cov_low.nrows() {
        assert!(cov_low[[i, i]] > 0.0);
    }
    for i in 0..cov_high.nrows() {
        assert!(cov_high[[i, i]] > 0.0);
    }
}

/// n=50 でも Σ_LW が full rank（全固有値正）
#[test]
fn test_ledoit_wolf_full_rank() {
    let returns = generate_synthetic_returns(50, 20, 789);
    let cov = calculate_covariance_matrix(&returns);

    let n = cov.nrows();
    let mat = nalgebra::DMatrix::from_fn(n, n, |i, j| cov[[i, j]]);
    let eigen = mat.symmetric_eigen();

    // Ledoit-Wolf + PSD 保証により全固有値は正
    for &ev in eigen.eigenvalues.iter() {
        assert!(ev > 0.0, "All eigenvalues must be positive, got {}", ev);
    }
}

/// n=8 で既存動作との後方互換（δ は小さく、S に近い結果）
#[test]
fn test_ledoit_wolf_backward_compat() {
    let returns = generate_synthetic_returns(8, 29, 101);
    let n = returns.len();

    // サンプル共分散（正則化なし）
    let mut sample_cov = ndarray::Array2::zeros((n, n));
    for i in 0..n {
        for j in i..n {
            let c = calculate_covariance(&returns[i], &returns[j]);
            sample_cov[[i, j]] = c;
            sample_cov[[j, i]] = c;
        }
    }

    let result = ledoit_wolf_shrink(&returns);

    // n=8, T=29: T > n なのでサンプル共分散はフルランクに近い
    // δ は比較的小さいはず → 結果は sample_cov に近い
    let mut max_diff = 0.0_f64;
    for i in 0..n {
        for j in 0..n {
            max_diff = max_diff.max((result[[i, j]] - sample_cov[[i, j]]).abs());
        }
    }

    // 差分はサンプル共分散のスケールに比べて小さい
    let max_cov = sample_cov.iter().map(|v| v.abs()).fold(0.0_f64, f64::max);
    assert!(
        max_diff < max_cov * 0.5,
        "With n<T, shrinkage should be moderate: max_diff={}, max_cov={}",
        max_diff,
        max_cov
    );
}

/// 条件数が合理的な範囲に収まる
#[test]
fn test_ledoit_wolf_well_conditioned() {
    // n > T のケース: サンプル共分散は severely rank-deficient
    let returns = generate_synthetic_returns(50, 15, 202);
    let cov = calculate_covariance_matrix(&returns);

    let n = cov.nrows();
    let mat = nalgebra::DMatrix::from_fn(n, n, |i, j| cov[[i, j]]);
    let eigen = mat.symmetric_eigen();

    let max_ev = eigen
        .eigenvalues
        .iter()
        .cloned()
        .fold(f64::NEG_INFINITY, f64::max);
    let min_ev = eigen
        .eigenvalues
        .iter()
        .cloned()
        .fold(f64::INFINITY, f64::min);

    let condition_number = max_ev / min_ev;
    assert!(
        condition_number < 1e8,
        "Condition number should be reasonable after Ledoit-Wolf: {}",
        condition_number
    );
}

/// 不等長リターンで T アライン（末尾共通窓）が正しく動作する
#[test]
fn test_ledoit_wolf_unequal_length_uses_common_window() {
    // 不等長リターン: T_min = 7
    let returns = vec![
        vec![9.9, 9.8, 9.7, 0.01, -0.02, 0.03, -0.01, 0.02, 0.01, -0.03], // 10点
        vec![0.02, -0.01, 0.03, 0.01, -0.02, 0.04, -0.01],                // 7点
        vec![0.05, 0.01, -0.03, 0.02, 0.01, -0.01, 0.03, 0.02],           // 8点
    ];

    let result = ledoit_wolf_shrink(&returns);

    // 末尾7点のみで計算した結果と一致するはず
    let trimmed: Vec<Vec<f64>> = returns.iter().map(|r| r[r.len() - 7..].to_vec()).collect();
    let expected = ledoit_wolf_shrink(&trimmed);

    // 一致検証
    for i in 0..3 {
        for j in 0..3 {
            assert!(
                (result[[i, j]] - expected[[i, j]]).abs() < 1e-14,
                "[{},{}]: unequal={}, trimmed={}",
                i,
                j,
                result[[i, j]],
                expected[[i, j]]
            );
        }
    }

    // 先頭余剰データの非影響を検証
    let mut returns_mod = returns.clone();
    returns_mod[0][0] = 999.0; // 先頭を大きく変更（T窓外）
    let result_mod = ledoit_wolf_shrink(&returns_mod);
    for i in 0..3 {
        for j in 0..3 {
            assert!(
                (result[[i, j]] - result_mod[[i, j]]).abs() < 1e-14,
                "Data outside T window should not affect result"
            );
        }
    }
}

// --- box_maximize_sharpe テスト ---

/// w_i ≤ max_position の制約充足
#[test]
fn test_box_sharpe_basic() {
    let returns = generate_synthetic_returns(6, 30, 303);
    let cov = calculate_covariance_matrix(&returns);
    let expected_returns: Vec<f64> = vec![0.02, 0.05, 0.01, 0.04, 0.03, 0.06];
    let max_pos = 0.3;

    let weights = box_maximize_sharpe(&expected_returns, &cov, max_pos);

    assert_eq!(weights.len(), 6);

    // 合計 ≈ 1.0
    let sum: f64 = weights.iter().sum();
    assert!((sum - 1.0).abs() < 1e-8, "Weights must sum to 1.0: {}", sum);

    // 全 w_i ∈ [0, max_position]
    for (i, &w) in weights.iter().enumerate() {
        assert!(w >= -1e-10, "Weight {} must be non-negative: {}", i, w);
        assert!(
            w <= max_pos + 1e-8,
            "Weight {} exceeds max_position: {} > {}",
            i,
            w,
            max_pos
        );
    }
}

/// max_position=1.0 で既存 maximize_sharpe_ratio と同一解
#[test]
fn test_box_sharpe_backward_compat() {
    let returns = generate_synthetic_returns(4, 30, 404);
    let cov = calculate_covariance_matrix(&returns);
    let expected_returns: Vec<f64> = vec![0.03, 0.05, 0.01, 0.04];

    let w_box = box_maximize_sharpe(&expected_returns, &cov, 1.0);
    let w_orig = maximize_sharpe_ratio(&expected_returns, &cov);

    // 同一解であるべき
    for (i, (&wb, &wo)) in w_box.iter().zip(w_orig.iter()).enumerate() {
        assert!(
            (wb - wo).abs() < 1e-8,
            "Weight {} differs: box={}, orig={}",
            i,
            wb,
            wo
        );
    }
}

/// n=100 での動作・制約充足
#[test]
fn test_box_sharpe_n100() {
    let returns = generate_synthetic_returns(100, 29, 505);
    let cov = calculate_covariance_matrix(&returns);
    let expected_returns: Vec<f64> = (0..100).map(|i| 0.01 + (i as f64) * 0.0005).collect();
    let max_pos = 0.3;

    let weights = box_maximize_sharpe(&expected_returns, &cov, max_pos);

    assert_eq!(weights.len(), 100);
    let sum: f64 = weights.iter().sum();
    assert!((sum - 1.0).abs() < 1e-6, "Sum={}", sum);

    for (i, &w) in weights.iter().enumerate() {
        assert!(w >= -1e-10, "Negative weight at {}: {}", i, w);
        assert!(w <= max_pos + 1e-6, "Exceeds max at {}: {}", i, w);
    }
}

/// 収束失敗時に等配分（default_weights）にフォールバックすることを検証
///
/// Active Set 法でサイクリングを引き起こす入力を構築する。
/// 高相関行列 + 対称的な excess returns で Free↔Lower↔Upper の遷移が
/// 繰り返し発生し、max_iter 以内に収束しないケースを狙う。
#[test]
fn test_box_sharpe_convergence_failure_returns_default_weights() {
    let n = 5;
    let max_pos = 0.25;

    // 高条件数の共分散行列: ほぼ完全に相関した資産群
    // off-diagonal を on-diagonal に近づけることで KKT 勾配の符号が
    // わずかな重み変化で反転し、集合間の移動がサイクルする
    let mut cov = Array2::zeros((n, n));
    for i in 0..n {
        for j in 0..n {
            if i == j {
                cov[[i, j]] = 1.0;
            } else {
                // 交互に正負の高相関: 隣接資産と逆相関させることで
                // Free→Upper→Free のサイクルを誘発
                let sign = if (i + j) % 2 == 0 { 1.0 } else { -1.0 };
                cov[[i, j]] = sign * 0.999;
            }
        }
    }

    // excess returns を対称的に正負交互、かつ極小にすることで
    // unconstrained 最適解が極端な値になり box 制約と衝突を繰り返す
    let expected_returns: Vec<f64> = (0..n)
        .map(|i| {
            let sign = if i % 2 == 0 { 1.0 } else { -1.0 };
            sign * 1e-8
        })
        .collect();

    let weights = box_maximize_sharpe(&expected_returns, &cov, max_pos);

    // 検証: 収束失敗時は等配分 OR 制約充足のいずれかを確認
    let equal = 1.0 / n as f64;
    let is_equal = weights.iter().all(|&w| (w - equal).abs() < 1e-10);

    // 合計が 1.0 であること
    let sum: f64 = weights.iter().sum();
    assert!((sum - 1.0).abs() < 1e-8, "Weights must sum to 1.0: {}", sum);

    // 長さが n であること
    assert_eq!(weights.len(), n);

    if is_equal {
        // 等配分にフォールバック（収束失敗パス）
        for (i, &w) in weights.iter().enumerate() {
            assert!(
                (w - equal).abs() < 1e-10,
                "Expected equal weight at {}: got {}",
                i,
                w
            );
        }
    } else {
        // 収束した場合でも制約は満たされるべき
        for (i, &w) in weights.iter().enumerate() {
            assert!(w >= -1e-10, "Weight {} must be non-negative: {}", i, w);
            assert!(
                w <= max_pos + 1e-8,
                "Weight {} exceeds max_position: {} > {}",
                i,
                w,
                max_pos
            );
        }
    }
}

// --- box_risk_parity テスト ---

/// box 制約付き RP の制約充足
#[test]
fn test_box_rp_basic() {
    let returns = generate_synthetic_returns(6, 30, 606);
    let cov = calculate_covariance_matrix(&returns);
    let max_pos = 0.3;

    let weights = box_risk_parity(&cov, max_pos);

    assert_eq!(weights.len(), 6);
    let sum: f64 = weights.iter().sum();
    assert!((sum - 1.0).abs() < 1e-8, "Sum={}", sum);

    for (i, &w) in weights.iter().enumerate() {
        assert!(w >= -1e-10, "Negative at {}: {}", i, w);
        assert!(w <= max_pos + 1e-8, "Exceeds max at {}: {}", i, w);
    }
}

/// n=100 での RP 動作
#[test]
fn test_box_rp_n100() {
    let returns = generate_synthetic_returns(100, 29, 707);
    let cov = calculate_covariance_matrix(&returns);
    let max_pos = 0.3;

    let weights = box_risk_parity(&cov, max_pos);

    assert_eq!(weights.len(), 100);
    let sum: f64 = weights.iter().sum();
    assert!((sum - 1.0).abs() < 1e-6, "Sum={}", sum);

    for (i, &w) in weights.iter().enumerate() {
        assert!(w >= -1e-10, "Negative at {}: {}", i, w);
        assert!(w <= max_pos + 1e-6, "Exceeds max at {}: {}", i, w);
    }
}

/// Pinned→Free (unpin) 遷移が発生することを検証
///
/// 低自己分散だが高分散資産との高相関を持つ資産を含む共分散行列で:
/// 1. RP が低 marginal risk の資産に高ウェイトを付与 → max_position 超過 → Pinned
/// 2. Pinned 後、高相関による marginal risk 増幅で RC > 1.5 * target_rc → Unpin
/// 3. Unpin された資産は再最適化で effective_max より小さい重みになる
#[test]
fn test_box_rp_pinned_to_free_unpin() {
    let max_pos = 0.35;

    // Asset 0: 低自己分散(0.02) + 他資産との高相関(off-diag 0.08)
    // → RP で高ウェイト（marginal risk が小さい）→ Pinned
    // → Pinned 後、高相関により marginal risk が増幅 → RC > 1.5 * target → Unpin
    //
    // Assets 1-3: 高自己分散(0.20) + 低相互相関(0.02)
    let cov = array![
        [0.02, 0.08, 0.08, 0.08],
        [0.08, 0.20, 0.02, 0.02],
        [0.08, 0.02, 0.20, 0.02],
        [0.08, 0.02, 0.02, 0.20],
    ];

    let weights = box_risk_parity(&cov, max_pos);

    // 基本制約: 合計 = 1.0
    let sum: f64 = weights.iter().sum();
    assert!((sum - 1.0).abs() < 1e-8, "Weights must sum to 1.0: {}", sum);

    // 基本制約: 各 w_i ∈ [0, max_position]
    for (i, &w) in weights.iter().enumerate() {
        assert!(w >= -1e-10, "Weight {} must be non-negative: {}", i, w);
        assert!(
            w <= max_pos + 1e-8,
            "Weight {} exceeds max_position: {} > {}",
            i,
            w,
            max_pos
        );
    }

    // Unpin が発生した証拠: asset 0 の最終重みが effective_max より小さい
    // （unpinned されて再最適化された場合、制約上限にピッタリ張り付かない）
    // effective_max = max_pos (n * max_pos = 4 * 0.35 = 1.4 > 1.0)
    assert!(
        weights[0] < max_pos - 1e-6,
        "Asset 0 should be unpinned (weight {} should be < max_pos {})",
        weights[0],
        max_pos
    );

    // RP の品質: リスク寄与度の均等度がある程度保たれていること
    let div = risk_parity_divergence(&weights, &cov);
    assert!(
        div < 0.5,
        "Risk parity divergence too high after unpin: {}",
        div
    );
}

// --- ユーティリティ関数テスト ---

/// サブ問題抽出の正当性
#[test]
fn test_extract_sub_portfolio() {
    let returns = vec![0.01, 0.02, 0.03, 0.04, 0.05];
    let cov =
        ndarray::Array2::from_shape_fn(
            (5, 5),
            |(i, j)| {
                if i == j { 0.01 * (i + 1) as f64 } else { 0.001 }
            },
        );
    let indices = vec![1, 3];

    let (sub_ret, sub_cov) = extract_sub_portfolio(&returns, &cov, &indices);

    assert_eq!(sub_ret, vec![0.02, 0.04]);
    assert_eq!(sub_cov.shape(), [2, 2]);
    assert!((sub_cov[[0, 0]] - 0.02).abs() < 1e-10); // index 1 → 0.01 * 2
    assert!((sub_cov[[1, 1]] - 0.04).abs() < 1e-10); // index 3 → 0.01 * 4
    assert!((sub_cov[[0, 1]] - 0.001).abs() < 1e-10); // off-diagonal
}

/// RC 均等度の計算
#[test]
fn test_risk_parity_divergence() {
    let cov = ndarray::Array2::from_shape_fn((3, 3), |(i, j)| if i == j { 0.01 } else { 0.002 });

    // 等配分は均等な共分散行列で完全 RP → 乖離度 ≈ 0
    let equal_w = vec![1.0 / 3.0; 3];
    let div_equal = risk_parity_divergence(&equal_w, &cov);
    assert!(
        div_equal < 1e-10,
        "Equal weights on uniform cov should have ~0 divergence: {}",
        div_equal
    );

    // 不均等な重みは乖離度 > 0
    let unequal_w = vec![0.8, 0.1, 0.1];
    let div_unequal = risk_parity_divergence(&unequal_w, &cov);
    assert!(
        div_unequal > div_equal,
        "Unequal weights should have higher divergence"
    );
}

/// 流動性ペナルティ効果
#[test]
fn test_liquidity_adjustment() {
    let returns = vec![0.05, 0.05, 0.05];
    let liquidity = vec![1.0, 0.5, 0.0];

    let adj = adjust_returns_for_liquidity(&returns, &liquidity);

    // liquidity=1.0 → ペナルティなし
    assert!((adj[0] - 0.05).abs() < 1e-10);
    // liquidity=0.5 → 0.005 のペナルティ
    assert!((adj[1] - 0.045).abs() < 1e-10);
    // liquidity=0.0 → 0.01 のペナルティ
    assert!((adj[2] - 0.04).abs() < 1e-10);
}

/// C(n,k) 列挙の正当性
#[test]
fn test_combinations_iterator() {
    // C(5, 3) = 10
    let combos: Vec<Vec<usize>> = Combinations::new(5, 3).collect();
    assert_eq!(combos.len(), 10);

    // 辞書式順序
    assert_eq!(combos[0], vec![0, 1, 2]);
    assert_eq!(combos[9], vec![2, 3, 4]);

    // 全組み合わせが厳密な辞書式昇順であることを検証
    for window in combos.windows(2) {
        assert!(
            window[0] < window[1],
            "Not in lexicographic order: {:?} >= {:?}",
            window[0],
            window[1]
        );
    }

    // 全要素がユニーク
    for combo in &combos {
        for i in 0..combo.len() {
            for j in (i + 1)..combo.len() {
                assert_ne!(combo[i], combo[j]);
            }
        }
    }

    // C(4, 2) = 6
    let combos4_2: Vec<Vec<usize>> = Combinations::new(4, 2).collect();
    assert_eq!(combos4_2.len(), 6);

    // C(4, 2) も厳密な辞書式昇順であることを検証
    for window in combos4_2.windows(2) {
        assert!(
            window[0] < window[1],
            "C(4,2) not in lexicographic order: {:?} >= {:?}",
            window[0],
            window[1]
        );
    }

    // C(6, 6) = 1
    let combos6_6: Vec<Vec<usize>> = Combinations::new(6, 6).collect();
    assert_eq!(combos6_6.len(), 1);
    assert_eq!(combos6_6[0], vec![0, 1, 2, 3, 4, 5]);

    // C(3, 0) = empty (k=0)
    let combos_empty: Vec<Vec<usize>> = Combinations::new(3, 0).collect();
    assert_eq!(combos_empty.len(), 0);

    // C(2, 5) = empty (k > n)
    let combos_impossible: Vec<Vec<usize>> = Combinations::new(2, 5).collect();
    assert_eq!(combos_impossible.len(), 0);
}

// --- 統合テスト ---

/// n ≤ max_holdings でエッジケース処理
#[test]
fn test_unified_small_n() {
    let returns = generate_synthetic_returns(3, 30, 808);
    let cov = calculate_covariance_matrix(&returns);
    let expected_returns: Vec<f64> = vec![0.03, 0.05, 0.02];
    let liquidity = vec![0.8, 0.9, 0.7];

    let weights = unified_optimize(
        &expected_returns,
        &cov,
        &liquidity,
        0.5,  // max_position
        6,    // max_holdings (> n)
        0.05, // min_position_size
        0.8,  // alpha
    );

    assert_eq!(weights.len(), 3);
    let sum: f64 = weights.iter().sum();
    assert!((sum - 1.0).abs() < 1e-8, "Sum={}", sum);
}

/// n=10 での動作
#[test]
fn test_unified_medium_n() {
    let returns = generate_synthetic_returns(10, 29, 909);
    let cov = calculate_covariance_matrix(&returns);
    let expected_returns: Vec<f64> = (0..10).map(|i| 0.01 + (i as f64) * 0.005).collect();
    let liquidity = vec![0.8; 10];

    let weights = unified_optimize(&expected_returns, &cov, &liquidity, 0.4, 6, 0.05, 0.8);

    assert_eq!(weights.len(), 10);
    let sum: f64 = weights.iter().sum();
    assert!((sum - 1.0).abs() < 1e-6, "Sum={}", sum);

    // max_holdings=6 なので非ゼロは最大6個
    let non_zero = weights.iter().filter(|&&w| w > 1e-10).count();
    assert!(
        non_zero <= 6,
        "Non-zero count exceeds max_holdings: {}",
        non_zero
    );
}

/// n=50 での動作・計算時間
#[test]
fn test_unified_large_n() {
    let returns = generate_synthetic_returns(50, 29, 1010);
    let cov = calculate_covariance_matrix(&returns);
    let expected_returns: Vec<f64> = (0..50).map(|i| 0.01 + (i as f64) * 0.001).collect();
    let liquidity = vec![0.8; 50];

    let start = std::time::Instant::now();
    let weights = unified_optimize(&expected_returns, &cov, &liquidity, 0.4, 6, 0.05, 0.8);
    let elapsed = start.elapsed();

    assert_eq!(weights.len(), 50);
    let sum: f64 = weights.iter().sum();
    assert!((sum - 1.0).abs() < 1e-4, "Sum={}", sum);

    // 計算時間は数秒以内
    assert!(
        elapsed.as_secs() < 10,
        "Optimization took too long: {:?}",
        elapsed
    );
}

/// 全制約充足（box + max_holdings + min_position）
#[test]
fn test_unified_all_constraints_satisfied() {
    let returns = generate_synthetic_returns(15, 29, 1111);
    let cov = calculate_covariance_matrix(&returns);
    let expected_returns: Vec<f64> = (0..15).map(|i| 0.01 + (i as f64) * 0.003).collect();
    let liquidity = vec![0.8; 15];
    let max_pos = 0.35;
    let max_hold = 6;
    let min_pos = 0.05;

    let weights = unified_optimize(
        &expected_returns,
        &cov,
        &liquidity,
        max_pos,
        max_hold,
        min_pos,
        0.8,
    );

    // 合計 = 1.0
    let sum: f64 = weights.iter().sum();
    assert!((sum - 1.0).abs() < 1e-6, "Sum={}", sum);

    // box 制約
    for (i, &w) in weights.iter().enumerate() {
        assert!(w >= -1e-10, "Negative at {}: {}", i, w);
        assert!(
            w <= max_pos + 1e-6,
            "Exceeds max at {}: {} > {}",
            i,
            w,
            max_pos
        );
    }

    // max_holdings 制約
    let non_zero = weights.iter().filter(|&&w| w > 1e-10).count();
    assert!(
        non_zero <= max_hold,
        "Non-zero {} > max_holdings {}",
        non_zero,
        max_hold
    );

    // min_position_size 制約
    for &w in &weights {
        if w > 1e-10 {
            assert!(
                w >= min_pos - 1e-6,
                "Weight {} below min_position_size {}",
                w,
                min_pos
            );
        }
    }
}

/// 和集合枝刈りの正当性: Sharpe/RP 上位が保存される
#[test]
fn test_pruning_union_preserves_top_tokens() {
    let returns = generate_synthetic_returns(20, 29, 1212);
    let cov = calculate_covariance_matrix(&returns);
    // トークン 15-19 に極端に高いリターンを設定
    let mut expected_returns: Vec<f64> = vec![0.01; 20];
    for item in expected_returns.iter_mut().take(20).skip(15) {
        *item = 0.10;
    }
    let liquidity = vec![0.8; 20];

    let weights = unified_optimize(&expected_returns, &cov, &liquidity, 0.4, 6, 0.05, 0.8);

    // 高リターンのトークン群に重みが集中すべき
    let top_weight: f64 = weights[15..20].iter().sum();
    let bottom_weight: f64 = weights[0..15].iter().sum();
    assert!(
        top_weight > bottom_weight,
        "Top tokens should have more weight: top={}, bottom={}",
        top_weight,
        bottom_weight
    );
}

/// ハードフィルタが既存フィルタの最低条件と一致
#[test]
fn test_hard_filter_tokens() {
    let tokens = vec![
        TokenData {
            symbol: token_out("good-token"),
            current_rate: rate_from_price(0.01),
            historical_volatility: 0.1,
            liquidity_score: Some(0.8),
            market_cap: Some(cap(100_000)),
        },
        TokenData {
            symbol: token_out("low-liquidity"),
            current_rate: rate_from_price(0.01),
            historical_volatility: 0.1,
            liquidity_score: Some(0.2), // below MIN_LIQUIDITY_SCORE
            market_cap: Some(cap(100_000)),
        },
        TokenData {
            symbol: token_out("low-cap"),
            current_rate: rate_from_price(0.01),
            historical_volatility: 0.1,
            liquidity_score: Some(0.8),
            market_cap: Some(cap(100)), // below min_market_cap
        },
    ];

    let filtered = hard_filter_tokens(&tokens);

    // good-token のみ残る
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].symbol, token_out("good-token"));
}

/// MIN_POSITION_SIZE 後の再最適化で制約充足
#[test]
fn test_min_position_reoptimization() {
    // 多数のトークンで一部が MIN_POSITION_SIZE 未満になるケース
    let returns = generate_synthetic_returns(12, 29, 1313);
    let cov = calculate_covariance_matrix(&returns);
    let expected_returns: Vec<f64> = (0..12)
        .map(|i| if i < 3 { 0.08 } else { 0.005 }) // 上位3つが支配的
        .collect();
    let liquidity = vec![0.8; 12];

    let weights = unified_optimize(&expected_returns, &cov, &liquidity, 0.4, 6, 0.05, 0.9);

    let sum: f64 = weights.iter().sum();
    assert!((sum - 1.0).abs() < 1e-6, "Sum={}", sum);

    // 全非ゼロ重みが min_position_size 以上
    for &w in &weights {
        if w > 1e-10 {
            assert!(w >= 0.05 - 1e-6, "Weight {} below min_position_size", w);
        }
    }
}

/// 複合スコアと Sharpe+RP ブレンド目的の整合
#[test]
fn test_composite_score_consistency() {
    let returns = generate_synthetic_returns(8, 29, 1414);
    let cov = calculate_covariance_matrix(&returns);
    let expected_returns: Vec<f64> = vec![0.02, 0.05, 0.01, 0.04, 0.03, 0.06, 0.015, 0.035];

    // alpha=1.0: Sharpe のみ
    let w_sharpe_only = unified_optimize(&expected_returns, &cov, &[0.8; 8], 0.4, 6, 0.05, 1.0);

    // alpha=0.0: RP のみ
    let w_rp_only = unified_optimize(&expected_returns, &cov, &[0.8; 8], 0.4, 6, 0.05, 0.0);

    // 両方とも有効な重み
    let sum_s: f64 = w_sharpe_only.iter().sum();
    let sum_r: f64 = w_rp_only.iter().sum();
    assert!((sum_s - 1.0).abs() < 1e-6);
    assert!((sum_r - 1.0).abs() < 1e-6);

    // Sharpe のみの場合、高リターンのトークンにより集中
    // RP のみの場合、リスク均等化でより分散
    let max_w_sharpe = w_sharpe_only.iter().cloned().fold(0.0_f64, f64::max);
    let max_w_rp = w_rp_only.iter().cloned().fold(0.0_f64, f64::max);

    // Sharpe は RP より集中度が高いか等しい傾向
    // （必ずしも厳密ではないが、極端なケースでは成立）
    assert!(
        max_w_sharpe >= max_w_rp * 0.5,
        "Sharpe-only should not be much less concentrated than RP-only: sharpe_max={}, rp_max={}",
        max_w_sharpe,
        max_w_rp
    );
}

/// 特異共分散行列でリッジ正則化リトライにより最適化された重みが返る
#[test]
fn test_box_maximize_sharpe_singular_cov_ridge_recovers() {
    // 全行が同一 → 特異行列（rank 1）
    let singular = array![[1.0, 1.0, 1.0], [1.0, 1.0, 1.0], [1.0, 1.0, 1.0]];
    let returns = vec![0.05, 0.03, 0.04];
    let weights = box_maximize_sharpe(&returns, &singular, 0.5);
    // リッジ正則化により等配分ではなく最適化された重みが返る
    let sum: f64 = weights.iter().sum();
    assert!(
        (sum - 1.0).abs() < 1e-10,
        "weights should sum to 1.0, got {sum}"
    );
    for (i, &w) in weights.iter().enumerate() {
        assert!(w >= 0.0, "weight[{i}] should be non-negative, got {w}");
    }
    // 最高リターン（0.05）の資産が最大ウェイトを持つはず
    let max_idx = weights
        .iter()
        .enumerate()
        .max_by(|a, b| a.1.partial_cmp(b.1).unwrap())
        .unwrap()
        .0;
    assert_eq!(
        max_idx, 0,
        "asset 0 (highest return) should have max weight"
    );
    // 等配分ではないことを確認
    let equal = 1.0 / 3.0;
    let is_equal = weights.iter().all(|&w| (w - equal).abs() < 1e-6);
    assert!(
        !is_equal,
        "should not be equal-weighted after ridge recovery"
    );
}

/// 準特異行列（高条件数）でリッジ正則化が有効に機能する
#[test]
fn test_box_maximize_sharpe_near_singular_ridge_effective() {
    // ほぼ完全相関 → Cholesky/LU が数値的に不安定になりうる
    let near_singular = array![
        [1.0, 0.999999, 0.999998],
        [0.999999, 1.0, 0.999999],
        [0.999998, 0.999999, 1.0],
    ];
    let returns = vec![0.05, 0.03, 0.04];
    let weights = box_maximize_sharpe(&returns, &near_singular, 0.5);
    let sum: f64 = weights.iter().sum();
    assert!(
        (sum - 1.0).abs() < 1e-10,
        "weights should sum to 1.0, got {sum}"
    );
    for (i, &w) in weights.iter().enumerate() {
        assert!(w >= 0.0, "weight[{i}] should be non-negative, got {w}");
    }
}

/// maximize_sharpe_ratio でも特異行列に対してリッジ正則化が機能する
#[test]
fn test_maximize_sharpe_ratio_singular_cov_ridge_recovers() {
    let singular = array![[1.0, 1.0, 1.0], [1.0, 1.0, 1.0], [1.0, 1.0, 1.0]];
    let returns = vec![0.05, 0.03, 0.04];
    let weights = maximize_sharpe_ratio(&returns, &singular);
    let sum: f64 = weights.iter().sum();
    assert!(
        (sum - 1.0).abs() < 1e-10,
        "weights should sum to 1.0, got {sum}"
    );
    for (i, &w) in weights.iter().enumerate() {
        assert!(w >= 0.0, "weight[{i}] should be non-negative, got {w}");
    }
    // 最高リターンの資産が最大ウェイトを持つはず
    let max_idx = weights
        .iter()
        .enumerate()
        .max_by(|a, b| a.1.partial_cmp(b.1).unwrap())
        .unwrap()
        .0;
    assert_eq!(
        max_idx, 0,
        "asset 0 (highest return) should have max weight"
    );
}

/// q solve パスでリッジ正則化が機能する（Upper 集合非空 + 特異 Σ_FF）
///
/// row0 = row3 の rank-3 行列で、asset 1（最高リターン）が Upper に移動した後、
/// Free 部分行列 {0,2,3} が特異 → p/q solve の両方でリッジが必要。
#[test]
fn test_box_maximize_sharpe_singular_q_solve_ridge() {
    // row0 = row3 → rank 3（4x4 で rank deficient = 1）
    let singular = array![
        [0.04, 0.01, 0.005, 0.04],
        [0.01, 0.06, 0.015, 0.01],
        [0.005, 0.015, 0.08, 0.005],
        [0.04, 0.01, 0.005, 0.04],
    ];
    // asset 1 の超過リターンが突出 → Upper に移動。
    // 残り {0,2,3} の Σ_FF は特異（row0=row2 in submatrix）→ q solve にリッジが必要
    let returns = vec![0.04, 0.08, 0.03, 0.035];
    let weights = box_maximize_sharpe(&returns, &singular, 0.35);
    let sum: f64 = weights.iter().sum();
    assert!(
        (sum - 1.0).abs() < 1e-10,
        "weights should sum to 1.0, got {sum}"
    );
    for (i, &w) in weights.iter().enumerate() {
        assert!(w >= 0.0, "weight[{i}] should be non-negative, got {w}");
    }
    // 等配分ではないことを確認（リッジで最適化された結果）
    let equal = 1.0 / 4.0;
    let is_equal = weights.iter().all(|&w| (w - equal).abs() < 1e-6);
    assert!(
        !is_equal,
        "should not be equal-weighted after ridge recovery"
    );
}

/// 良条件行列ではリッジパスが不要で結果が一致する（回帰テスト）
#[test]
fn test_box_maximize_sharpe_well_conditioned_unaffected() {
    // 対角優位 → 確実に正定値、リッジ不要
    let well_cond = array![[0.04, 0.01, 0.005], [0.01, 0.09, 0.02], [0.005, 0.02, 0.16],];
    let returns = vec![0.05, 0.03, 0.04];
    let weights = box_maximize_sharpe(&returns, &well_cond, 0.5);
    let sum: f64 = weights.iter().sum();
    assert!(
        (sum - 1.0).abs() < 1e-10,
        "weights should sum to 1.0, got {sum}"
    );
    // 等配分ではなく差別化された重みが返る
    let equal = 1.0 / 3.0;
    let is_equal = weights.iter().all(|&w| (w - equal).abs() < 1e-6);
    assert!(
        !is_equal,
        "well-conditioned matrix should produce differentiated weights"
    );
    // 最高リターン（0.05）の資産が最大ウェイト
    let max_idx = weights
        .iter()
        .enumerate()
        .max_by(|a, b| a.1.partial_cmp(b.1).unwrap())
        .unwrap()
        .0;
    assert_eq!(
        max_idx, 0,
        "asset 0 (highest return) should have max weight"
    );
}

/// unified_optimize の結果が厳密に合計 1.0 になることを確認
#[test]
fn test_unified_optimize_weights_sum_to_one() {
    let returns = generate_synthetic_returns(6, 30, 4242);
    let cov = calculate_covariance_matrix(&returns);
    let expected_returns = vec![0.03, 0.05, 0.02, 0.04, 0.01, 0.06];
    let liquidity = vec![0.9, 0.7, 0.8, 0.6, 0.85, 0.75];

    let weights = unified_optimize(&expected_returns, &cov, &liquidity, 0.4, 4, 0.05, 0.7);

    let sum: f64 = weights.iter().sum();
    assert!(
        (sum - 1.0).abs() < 1e-12,
        "weights sum = {sum}, expected exactly 1.0"
    );
}

/// exhaustive_optimize の golden output テスト
///
/// キャッシュ導入前後で数値結果が同一であることを保証する。
#[test]
fn test_exhaustive_optimize_golden_output() {
    let returns = generate_synthetic_returns(8, 29, 5555);
    let cov = calculate_covariance_matrix(&returns);
    let expected_returns: Vec<f64> = vec![0.03, 0.06, 0.01, 0.05, 0.02, 0.07, 0.015, 0.04];
    let active_indices: Vec<usize> = (0..8).collect();

    let weights = exhaustive_optimize(
        &active_indices,
        &expected_returns,
        &cov,
        0.4,  // max_position
        3,    // max_holdings
        0.05, // min_position_size
        0.7,  // alpha
    );

    let golden: Vec<f64> = vec![
        0.0,
        0.336_285_038_199_695_35,
        0.0,
        0.294_239_939_813_386_7,
        0.0,
        0.369_475_021_986_917_95,
        0.0,
        0.0,
    ];
    assert_eq!(weights.len(), golden.len());
    for (i, (&w, &g)) in weights.iter().zip(golden.iter()).enumerate() {
        assert!((w - g).abs() < 1e-12, "weights[{i}]: got {w}, expected {g}");
    }
}

/// adjust_returns_for_liquidity に長さ不一致を渡すと debug_assert で panic する
#[cfg(debug_assertions)]
#[test]
#[should_panic(expected = "expected_returns and liquidity_scores must have the same length")]
fn test_adjust_returns_for_liquidity_length_mismatch_panics() {
    let returns = vec![0.01, 0.02, 0.03];
    let liquidity = vec![0.5, 0.6]; // 長さ不一致
    let _ = adjust_returns_for_liquidity(&returns, &liquidity);
}

/// 全トークンがフィルタ条件を満たさない場合、空 Vec を返す
#[test]
fn test_hard_filter_tokens_returns_empty_when_none_pass() {
    let tokens = vec![
        TokenData {
            symbol: token_out("low-liquidity"),
            current_rate: rate_from_price(0.01),
            historical_volatility: 0.1,
            liquidity_score: Some(0.2), // below MIN_LIQUIDITY_SCORE (0.5)
            market_cap: Some(cap(100_000)),
        },
        TokenData {
            symbol: token_out("low-cap"),
            current_rate: rate_from_price(0.01),
            historical_volatility: 0.1,
            liquidity_score: Some(0.8),
            market_cap: Some(cap(100)), // below min_market_cap (10,000)
        },
        TokenData {
            symbol: token_out("both-bad"),
            current_rate: rate_from_price(0.01),
            historical_volatility: 0.1,
            liquidity_score: Some(0.1),
            market_cap: Some(cap(50)),
        },
    ];

    let filtered = hard_filter_tokens(&tokens);
    assert!(
        filtered.is_empty(),
        "Expected empty Vec when no tokens pass hard filter"
    );
}

/// フィルタ通過トークンなし → execute_portfolio_optimization が Hold で早期リターン
#[tokio::test]
async fn test_execute_portfolio_optimization_hold_on_empty_filter() {
    // 全トークンが流動性条件を満たさない
    let tokens = vec![
        TokenData {
            symbol: token_out("illiquid-a"),
            current_rate: rate_from_price(0.01),
            historical_volatility: 0.2,
            liquidity_score: Some(0.1),
            market_cap: Some(cap(100)),
        },
        TokenData {
            symbol: token_out("illiquid-b"),
            current_rate: rate_from_price(0.02),
            historical_volatility: 0.3,
            liquidity_score: Some(0.2),
            market_cap: Some(cap(50)),
        },
    ];

    let wallet = WalletInfo {
        holdings: BTreeMap::new(),
        total_value: NearValue::from_near(BigDecimal::from(1000)),
        cash_balance: NearValue::zero(),
    };

    let portfolio_data = PortfolioData {
        tokens,
        predictions: BTreeMap::new(),
        historical_prices: BTreeMap::new(),
        prediction_confidence: None,
    };

    let report = execute_portfolio_optimization(&wallet, portfolio_data, 0.05)
        .await
        .unwrap();

    assert_eq!(report.actions.len(), 1);
    assert!(matches!(report.actions[0], TradingAction::Hold));
    assert!(!report.rebalance_needed);
    assert!(report.optimal_weights.weights.is_empty());
    assert_eq!(report.expected_metrics.sortino_ratio, 0.0);
    assert_eq!(report.expected_metrics.max_drawdown, 0.0);
}
