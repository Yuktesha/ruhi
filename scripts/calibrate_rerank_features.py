#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
如喜（RuhI）Stage D1-B：dev 集 Top-32 N-Best 5 折交叉驗證（5-Fold OOF CV）多特徵重排融合器校準腳本

設計原則：
1. 僅使用純 Python + NumPy（無外部 ML 套件依賴）。
2. 嚴格以 `fold = idx % 5` 切分，對每一折僅用 4/5 訓練資料擬合多特徵權重，
   並於完全未見之留出 1/5 折計算 Out-of-Fold (OOF) CER、整句命中率、fixed/broken 與五大文體 CER。
3. 精確模擬現行執行期約束：`score_margin = 25.0` 及 `PrefixTrieBudget(max_trie_nodes = 200)`。
"""
from __future__ import annotations

import argparse
import json
import math
from pathlib import Path
import sys
from typing import Any

import numpy as np

if sys.platform == "win32":
    try:
        sys.stdout.reconfigure(encoding="utf-8")
        sys.stderr.reconfigure(encoding="utf-8")
    except Exception:
        pass

FEATURE_NAMES = [
    "base_score",
    "neural_logp",
    "neural_min_char_logp",
    "word_count",
    "single_char_count",
    "log1p_min_node_weight",
    "is_top1",
    "discourse_delta",
    "neural_logp_per_char",
]


def char_levenshtein(a: str, b: str) -> int:
    if a == b:
        return 0
    la, lb = len(a), len(b)
    if la == 0:
        return lb
    if lb == 0:
        return la
    if la == lb:
        # 常見等長替換先檢查 Hamming 距離上界（但仍精確跑 DP 若含插入刪除）
        pass
    prev = list(range(lb + 1))
    for i, ca in enumerate(a, 1):
        curr = [i]
        for j, cb in enumerate(b, 1):
            cost = 0 if ca == cb else 1
            curr.append(min(curr[j - 1] + 1, prev[j] + 1, prev[j - 1] + cost))
        prev = curr
    return prev[lb]


def select_active_indices(
    cands: list[dict[str, Any]],
    score_margin: float = 25.0,
    max_trie_nodes: int = 200,
) -> list[int]:
    """對齊 `neural_reranker.rs::rerank_nbest_if_enabled` 候選篩選邏輯（base_score 降序 + margin + Trie 200 節點預算）。"""
    if len(cands) < 2:
        return list(range(len(cands)))
    top1_sc = float(cands[0]["base_score"])
    # Trie non-leaf budget simulation (base_prefix_nodes = 1 for empty prefix)
    children: list[dict[str, int]] = [{}]
    cur_nodes = 1
    kept: list[int] = []
    for idx, c in enumerate(cands):
        sc = float(c["base_score"])
        if top1_sc - sc > score_margin:
            break
        txt = str(c["text"])
        # count additional non-leaf nodes (first len-1 chars)
        node = 0
        diverged = False
        extra = 0
        for ch in txt[:-1]:
            if not diverged and ch in children[node]:
                node = children[node][ch]
            else:
                diverged = True
                extra += 1
        if len(kept) >= 2 and cur_nodes + extra > max_trie_nodes:
            continue
        node = 0
        for ch in txt[:-1]:
            if ch not in children[node]:
                nxt = len(children)
                children.append({})
                children[node][ch] = nxt
            node = children[node][ch]
        cur_nodes = 1 + (len(children) - 1)
        kept.append(idx)
    return kept


def extract_features(c: dict[str, Any], top1_text: str, idx: int) -> np.ndarray:
    txt = str(c["text"])
    char_len = max(1, len(txt))
    nlp = float(c.get("neural_logp", 0.0))
    min_nlp = float(c.get("neural_min_char_logp", nlp / char_len))
    wc = float(c.get("word_count", 1.0))
    sc_cnt = float(c.get("single_char_count", 0.0))
    min_w = float(c.get("min_node_weight", 0.0))
    is_top1 = 1.0 if (idx == 0 or txt == top1_text) else 0.0
    d_delta = float(c.get("discourse_delta", 0.0))
    return np.array(
        [
            float(c["base_score"]),
            nlp,
            min_nlp,
            wc,
            sc_cnt,
            math.log1p(max(0.0, min_w)),
            is_top1,
            d_delta,
            nlp / char_len,
        ],
        dtype=np.float64,
    )


def fit_pairwise_adam(
    diff_x: np.ndarray,
    pair_weights: np.ndarray,
    l2_reg: float,
    prior_std_weights: np.ndarray,
    steps: int = 500,
    lr: float = 0.05,
) -> np.ndarray:
    """對 `(x_good - x_bad)` 以加權邏輯損失＋對 prior 權重之 L2 正則擬合線性融合權重。"""
    n_feat = diff_x.shape[1]
    w = prior_std_weights.copy()
    m = np.zeros(n_feat, dtype=np.float64)
    v = np.zeros(n_feat, dtype=np.float64)
    total_pw = max(1.0, float(np.sum(pair_weights)))
    b1, b2, eps = 0.9, 0.999, 1e-8

    for t in range(1, steps + 1):
        logits = np.clip(diff_x @ w, -30.0, 30.0)
        sig_neg = 1.0 / (1.0 + np.exp(logits))  # P(mistake) = sigmoid(-logits)
        grad = -(diff_x.T @ (pair_weights * sig_neg)) / total_pw + l2_reg * (w - prior_std_weights)
        # 保持 base_score 權重恆為嚴格正值，避免符號翻轉
        m = b1 * m + (1.0 - b1) * grad
        v = b2 * v + (1.0 - b2) * (grad * grad)
        m_hat = m / (1.0 - b1**t)
        v_hat = v / (1.0 - b2**t)
        w = w - lr * m_hat / (np.sqrt(v_hat) + eps)
        if w[0] < 0.05:
            w[0] = 0.05
    return w


def evaluate_predictions(rows: list[dict[str, Any]], chosen_indices: list[int]) -> dict[str, Any]:
    total_chars = 0
    total_errs = 0
    exact = 0
    off_errs = 0
    off_exact = 0
    fixed = 0
    broken = 0
    cat_chars: dict[str, int] = {}
    cat_errs: dict[str, int] = {}

    for row, ci in zip(rows, chosen_indices):
        chars = row["chars"]
        total_chars += chars
        e = row["errors"][ci]
        off_e = row["off_err"]
        total_errs += e
        off_errs += off_e
        if e == 0:
            exact += 1
        if off_e == 0:
            off_exact += 1
        if off_e > 0 and e == 0:
            fixed += 1
        elif off_e == 0 and e > 0:
            broken += 1
        cat = row["category"]
        cat_chars[cat] = cat_chars.get(cat, 0) + chars
        cat_errs[cat] = cat_errs.get(cat, 0) + e

    return {
        "cer_pct": total_errs * 100.0 / max(1, total_chars),
        "char_errors": total_errs,
        "total_chars": total_chars,
        "exact": exact,
        "total_sents": len(rows),
        "exact_pct": exact * 100.0 / max(1, len(rows)),
        "fixed": fixed,
        "broken": broken,
        "net_vs_off": off_errs - total_errs,
        "per_category_cer": {
            k: round(cat_errs[k] * 100.0 / max(1, cat_chars[k]), 2)
            for k in sorted(cat_chars)
        },
        "per_category_errs": {k: cat_errs[k] for k in sorted(cat_errs)},
    }


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("jsonl_path", type=Path)
    parser.add_argument("--margin", type=float, default=25.0)
    parser.add_argument("--max-trie-nodes", type=int, default=200)
    args = parser.parse_args()

    rows: list[dict[str, Any]] = []
    with args.jsonl_path.open("r", encoding="utf-8") as f:
        for line_idx, line in enumerate(f):
            if not line.strip():
                continue
            r = json.loads(line)
            gold = r["gold"]
            top1 = r["top1"]
            cands = r["candidates"]
            active = select_active_indices(cands, args.margin, args.max_trie_nodes)
            if not active:
                continue
            feats = np.stack([extract_features(cands[i], top1, i) for i in active], axis=0)
            errs = [char_levenshtein(cands[i]["text"], gold) for i in active]
            rows.append(
                {
                    "id": r["id"],
                    "fold": line_idx % 5,
                    "category": r["category"],
                    "chars": len(gold),
                    "off_err": char_levenshtein(top1, gold),
                    "feats": feats,
                    "errors": errs,
                    "active": active,
                }
            )

    print(f"[載入完成] 有效句子數={len(rows)}, 總字數={sum(r['chars'] for r in rows)}")

    # 1. 對照組：現行純量融合 base_score + 2.25 * neural_logp
    base_chosen = []
    for r in rows:
        sc = r["feats"][:, 0] + 2.25 * r["feats"][:, 1]
        base_chosen.append(int(np.argmax(sc)))
    base_rep = evaluate_predictions(rows, base_chosen)
    print(
        f"[對照組 scalar lam=2.25] CER={base_rep['cer_pct']:.2f}% ({base_rep['char_errors']}/{base_rep['total_chars']}) | "
        f"exact={base_rep['exact']}/{base_rep['total_sents']} ({base_rep['exact_pct']:.2f}%) | "
        f"fixed={base_rep['fixed']} broken={base_rep['broken']} | cat={base_rep['per_category_errs']}"
    )

    # 2. 掃描 L2 正則強度執行嚴格 5 折交叉驗證
    best_oof_errs = 10**9
    best_cfg = None
    best_fold_weights: list[np.ndarray] = []

    for l2_reg in [0.002, 0.01, 0.05, 0.2, 1.0]:
        oof_chosen = [-1] * len(rows)
        fold_weights_orig: list[np.ndarray] = []
        for fold in range(5):
            train_rows = [r for r in rows if r["fold"] != fold]
            all_feats = np.concatenate([r["feats"] for r in train_rows], axis=0)
            std = np.maximum(np.std(all_feats, axis=0), 1e-6)

            # Pairwise differences in standardized feature space
            diff_list = []
            pw_list = []
            for r in train_rows:
                errs = r["errors"]
                min_e = min(errs)
                max_e = max(errs)
                if min_e == max_e:
                    continue
                f_std = r["feats"] / std
                good_idxs = [i for i, e in enumerate(errs) if e == min_e]
                bad_idxs = [i for i, e in enumerate(errs) if e > min_e]
                # 優先選取離線排名最前之正解／近似正解作為正樣本，涵蓋所有劣樣本
                gi = good_idxs[0]
                for bi in bad_idxs:
                    diff_list.append(f_std[gi] - f_std[bi])
                    pw_list.append(float(errs[bi] - min_e))
            diff_x = np.stack(diff_list, axis=0)
            pair_w = np.array(pw_list, dtype=np.float64)

            # Prior centered at baseline [1.0, 2.25, 0, ...] * std (scaled down to gentle logit scale ~0.08)
            raw_prior = np.zeros(len(FEATURE_NAMES), dtype=np.float64)
            raw_prior[0] = 1.0
            raw_prior[1] = 2.25
            prior_std = raw_prior * std * 0.08

            w_std = fit_pairwise_adam(diff_x, pair_w, l2_reg=l2_reg, prior_std_weights=prior_std)
            w_orig = w_std / std
            # Normalize scale so w_orig[0] == 1.0 (base_score coefficient fixed to 1.0 for readability)
            w_orig = w_orig / max(1e-6, w_orig[0])
            fold_weights_orig.append(w_orig)

            for idx, r in enumerate(rows):
                if r["fold"] == fold:
                    scores = r["feats"] @ w_orig
                    oof_chosen[idx] = int(np.argmax(scores))

        oof_rep = evaluate_predictions(rows, oof_chosen)
        print(
            f"[5-Fold OOF l2={l2_reg:<5g}] CER={oof_rep['cer_pct']:.2f}% ({oof_rep['char_errors']}/{oof_rep['total_chars']}) | "
            f"exact={oof_rep['exact']}/{oof_rep['total_sents']} ({oof_rep['exact_pct']:.2f}%) | "
            f"fixed={oof_rep['fixed']} broken={oof_rep['broken']} | "
            f"delta_vs_scalar={oof_rep['char_errors'] - base_rep['char_errors']:+d} ch | cat={oof_rep['per_category_errs']}"
        )
        if oof_rep["char_errors"] < best_oof_errs:
            best_oof_errs = oof_rep["char_errors"]
            best_cfg = (l2_reg, oof_rep)
            best_fold_weights = fold_weights_orig

    if best_cfg is not None:
        l2_best, best_rep = best_cfg
        mean_w = np.mean(np.stack(best_fold_weights, axis=0), axis=0)
        std_w = np.std(np.stack(best_fold_weights, axis=0), axis=0)
        print("\n=== 最佳 5 折交叉驗證結果 ===")
        print(
            f"  最佳 L2={l2_best}: OOF CER={best_rep['cer_pct']:.2f}% ({best_rep['char_errors']} errors, "
            f"delta={best_rep['char_errors'] - base_rep['char_errors']:+d}), broken={best_rep['broken']} (vs {base_rep['broken']})"
        )
        print("  平均特徵權重 (已正規化 base_score = 1.0000):")
        for name, mw, sw in zip(FEATURE_NAMES, mean_w, std_w):
            print(f"    {name:24s}: {mw:+9.4f} (±{sw:.4f})")

        # 額外掃描單純在既有公式外加 top1 防誤改 bonus 的敏感度（純對照）
        print("\n=== top1_bonus 防誤改安全門檻對照（不更動其餘權重） ===")
        for bonus in [0.5, 1.0, 1.5, 2.0, 3.0]:
            ch_idx = []
            for r in rows:
                sc = r["feats"][:, 0] + 2.25 * r["feats"][:, 1] + bonus * r["feats"][:, 6]
                ch_idx.append(int(np.argmax(sc)))
            rep_b = evaluate_predictions(rows, ch_idx)
            print(
                f"  top1_bonus={bonus:.1f}: CER={rep_b['cer_pct']:.2f}% ({rep_b['char_errors']}) | "
                f"fixed={rep_b['fixed']} broken={rep_b['broken']}"
            )


if __name__ == "__main__":
    main()
