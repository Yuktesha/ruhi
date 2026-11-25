#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
如喜（RuhI）v1.0 輕量神經重排序模型訓練、INT8 量化與 RLNM0001 匯出管線 (train_neural_reranker.py)

依據 docs/RUHI_V1_NEURAL_RERANKER_SPEC.md 實作：
1. --self-test-format（無需 PyTorch，僅使用標準函式庫與 NumPy）：
   驗證 RLNM0001 二進位格式封裝、8-byte 對齊、FNV-1a 校驗、INT8 反量化誤差、
   因果遮罩（Causal Mask）不變量及前綴計分一致性。
2. --prepare-corpus / --train（於 Google Colab GPU 環境執行）：
   複用 scripts/train_ngram_lm.py 之 12 組商業安全台灣正體語料來源與 TaiwanPurityFilter
   （含 --dev-hashes 評估集排重），訓練字元級 Decoder-only Transformer（RuhI-CharLM-v1：
   RMSNorm + RoPE + SwiGLU + Weight-Tied LM Head，預設 ~22.59M 參數），並匯出
   Per-Row 對稱 INT8 量化之 RLNM0001 二進位檔（~22.8 MB）。
3. --eval-nbest：
   讀取 `ruhi_eval --set dev --dump-nbest` 產生之 Top-16 N-Best JSONL，
   掃描融合權重 lambda_neural 並輸出重排前後 CER、整句命中率與修復／退步統計。
"""
from __future__ import annotations

import argparse
from collections import Counter
import hashlib
import json
import math
import os
from pathlib import Path
import struct
import sys
import tempfile
import time
from typing import Any

import numpy as np

if sys.platform == "win32":
    try:
        sys.stdout.reconfigure(encoding="utf-8")
        sys.stderr.reconfigure(encoding="utf-8")
    except Exception:
        pass

RLNM_MAGIC = b"RLNM0001"
RLNM_VERSION = 1
RLNM_HEADER_BYTES = 128

FLAG_WEIGHT_TIED = 1 << 0
FLAG_ROPE = 1 << 1
FLAG_SWIGLU = 1 << 2
DEFAULT_FLAGS = FLAG_WEIGHT_TIED | FLAG_ROPE | FLAG_SWIGLU

PAD_TOKEN = "<pad>"
BOS_TOKEN = "<bos>"
EOS_TOKEN = "<eos>"
UNK_TOKEN = "<unk>"
SPECIAL_TOKENS = [PAD_TOKEN, BOS_TOKEN, EOS_TOKEN, UNK_TOKEN]
PAD_ID = 0
BOS_ID = 1
EOS_ID = 2
UNK_ID = 3

COMMON_PUNCT_CHARS = "，。！？；：「」『』（）—…、‧《》〈〉"


# ── 工具函式 ──────────────────────────────────────────────────────────────────

def align8(n: int) -> int:
    return (n + 7) & ~7


def fnv1a_64(data: bytes) -> int:
    h = 0xCBF29CE484222325
    prime = 0x100000001B3
    mask = 0xFFFFFFFFFFFFFFFF
    for b in data:
        h ^= b
        h = (h * prime) & mask
    return h


def quantize_per_row_int8(weight: np.ndarray) -> tuple[np.ndarray, np.ndarray]:
    """對 [R, C] 權重矩陣執行列向對稱 INT8 量化，回傳 (q_i8, scales_f32)。"""
    w = np.asarray(weight, dtype=np.float32)
    if w.ndim != 2:
        raise ValueError(f"Expected 2D matrix for per-row INT8 quantization, got shape={w.shape}")
    max_abs = np.max(np.abs(w), axis=1)
    scales = np.where(max_abs > 1e-8, max_abs / 127.0, 1.0).astype(np.float32)
    q = np.clip(np.rint(w / scales[:, None]), -127, 127).astype(np.int8)
    return q, scales


def char_levenshtein(a: str, b: str) -> int:
    if a == b:
        return 0
    la, lb = len(a), len(b)
    if la == 0:
        return lb
    if lb == 0:
        return la
    prev = list(range(lb + 1))
    for i, ca in enumerate(a, 1):
        curr = [i]
        for j, cb in enumerate(b, 1):
            cost = 0 if ca == cb else 1
            curr.append(min(curr[j - 1] + 1, prev[j] + 1, prev[j - 1] + cost))
        prev = curr
    return prev[lb]


# ── RLNM0001 二進位寫入與 NumPy 讀取推論器 ─────────────────────────────────────

def write_rlnm0001(
    out_path: str | Path,
    vocab: list[str],
    cfg: dict[str, Any],
    tensors: dict[str, np.ndarray],
    corpus_fingerprint: int = 0,
) -> dict[str, Any]:
    """將模型詞表與權重序列化為 8-byte 對齊且含 FNV-1a 校驗碼之 RLNM0001 二進位檔。"""
    vocab_size = len(vocab)
    d_model = int(cfg["d_model"])
    n_layers = int(cfg["n_layers"])
    n_heads = int(cfg["n_heads"])
    d_ff = int(cfg["d_ff"])
    max_seq_len = int(cfg["max_seq_len"])
    rope_theta = float(cfg.get("rope_theta", 10000.0))
    rms_eps = float(cfg.get("rms_eps", 1e-5))
    flags = int(cfg.get("flags", DEFAULT_FLAGS))

    if d_model % n_heads != 0:
        raise ValueError(f"d_model ({d_model}) must be divisible by n_heads ({n_heads})")
    if (d_model // n_heads) % 2 != 0:
        raise ValueError("head_dim must be even for RoPE")

    payload = bytearray()

    def append_aligned(raw: bytes) -> int:
        off = RLNM_HEADER_BYTES + len(payload)
        assert off % 8 == 0, f"Unaligned offset {off}"
        payload.extend(raw)
        pad = align8(len(raw)) - len(raw)
        if pad:
            payload.extend(b"\x00" * pad)
        return off

    # 1. 詞表字串區與索引區
    str_buf = bytearray()
    meta_buf = bytearray()
    for tok in vocab:
        tb = tok.encode("utf-8")
        meta_buf.extend(struct.pack("<II", len(str_buf), len(tb)))
        str_buf.extend(tb)

    off_vocab_meta = append_aligned(bytes(meta_buf))
    off_vocab_strings = append_aligned(bytes(str_buf))

    # 2. 詞嵌入（Weight-tied）與 Final RMSNorm
    emb_q, emb_s = quantize_per_row_int8(tensors["tok_emb"])
    if emb_q.shape != (vocab_size, d_model):
        raise ValueError(f"tok_emb shape mismatch: {emb_q.shape} vs {(vocab_size, d_model)}")
    off_tok_emb_q = append_aligned(emb_q.tobytes(order="C"))
    off_tok_emb_s = append_aligned(emb_s.astype("<f4").tobytes(order="C"))

    norm_final = np.asarray(tensors["norm_final"], dtype="<f4")
    if norm_final.shape != (d_model,):
        raise ValueError(f"norm_final shape mismatch: {norm_final.shape}")
    off_norm_final = append_aligned(norm_final.tobytes(order="C"))

    # 3. 逐層寫入權重
    off_layers = RLNM_HEADER_BYTES + len(payload)
    assert off_layers % 8 == 0
    layer_specs = [
        ("wq", (d_model, d_model)),
        ("wk", (d_model, d_model)),
        ("wv", (d_model, d_model)),
        ("wo", (d_model, d_model)),
        ("w_gate", (d_ff, d_model)),
        ("w_up", (d_ff, d_model)),
        ("w_down", (d_model, d_ff)),
    ]
    for li in range(n_layers):
        attn_norm = np.asarray(tensors[f"layers.{li}.attn_norm"], dtype="<f4")
        ffn_norm = np.asarray(tensors[f"layers.{li}.ffn_norm"], dtype="<f4")
        if attn_norm.shape != (d_model,) or ffn_norm.shape != (d_model,):
            raise ValueError(f"Layer {li} norm shape mismatch")
        append_aligned(attn_norm.tobytes(order="C"))
        append_aligned(ffn_norm.tobytes(order="C"))
        for lname, expected_shape in layer_specs:
            mat = tensors[f"layers.{li}.{lname}"]
            if tuple(mat.shape) != expected_shape:
                raise ValueError(f"layers.{li}.{lname} shape {mat.shape} != {expected_shape}")
            mq, ms = quantize_per_row_int8(mat)
            append_aligned(mq.tobytes(order="C"))
            append_aligned(ms.astype("<f4").tobytes(order="C"))

    payload_bytes = bytes(payload)
    total_bytes = RLNM_HEADER_BYTES + len(payload_bytes)
    payload_hash = fnv1a_64(payload_bytes)

    header_main = struct.pack(
        "<8sIIIIIIIIdQfI",
        RLNM_MAGIC,
        RLNM_VERSION,
        flags,
        vocab_size,
        d_model,
        n_layers,
        n_heads,
        d_ff,
        max_seq_len,
        rope_theta,
        corpus_fingerprint & 0xFFFFFFFFFFFFFFFF,
        rms_eps,
        0,
    )
    assert len(header_main) == 64

    header_dir = struct.pack(
        "<8Q",
        off_vocab_meta,
        off_vocab_strings,
        off_tok_emb_q,
        off_tok_emb_s,
        off_norm_final,
        off_layers,
        total_bytes,
        payload_hash,
    )
    assert len(header_dir) == 64

    out_p = Path(out_path)
    out_p.parent.mkdir(parents=True, exist_ok=True)
    with open(out_p, "wb") as f:
        f.write(header_main)
        f.write(header_dir)
        f.write(payload_bytes)

    return {
        "path": str(out_p),
        "total_bytes": total_bytes,
        "mb": round(total_bytes / (1024.0 * 1024.0), 4),
        "vocab_size": vocab_size,
        "d_model": d_model,
        "n_layers": n_layers,
        "n_heads": n_heads,
        "d_ff": d_ff,
        "max_seq_len": max_seq_len,
        "payload_fnv64": f"{payload_hash:#018x}",
    }


def load_rlnm0001_numpy(bin_path: str | Path) -> dict[str, Any]:
    """以純 NumPy 讀取並嚴格校驗 RLNM0001 檔案，反量化為可供參考推論之權重字典。"""
    raw = Path(bin_path).read_bytes()
    if len(raw) < RLNM_HEADER_BYTES:
        raise ValueError("RLNM0001 file truncated before 128-byte header")

    (
        magic,
        version,
        flags,
        vocab_size,
        d_model,
        n_layers,
        n_heads,
        d_ff,
        max_seq_len,
        rope_theta,
        corpus_fp,
        rms_eps,
        _reserved,
    ) = struct.unpack_from("<8sIIIIIIIIdQfI", raw, 0)

    if magic != RLNM_MAGIC or version != RLNM_VERSION:
        raise ValueError(f"Invalid magic/version: {magic!r} v{version}")

    (
        off_vocab_meta,
        off_vocab_strings,
        off_tok_emb_q,
        off_tok_emb_s,
        off_norm_final,
        off_layers,
        total_bytes,
        payload_hash,
    ) = struct.unpack_from("<8Q", raw, 64)

    if total_bytes != len(raw):
        raise ValueError(f"File size mismatch: header={total_bytes}, actual={len(raw)}")
    actual_hash = fnv1a_64(raw[RLNM_HEADER_BYTES:])
    if actual_hash != payload_hash:
        raise ValueError(f"Payload checksum mismatch: expected={payload_hash:#x}, actual={actual_hash:#x}")

    for off_val in (
        off_vocab_meta,
        off_vocab_strings,
        off_tok_emb_q,
        off_tok_emb_s,
        off_norm_final,
        off_layers,
    ):
        if off_val % 8 != 0 or off_val < RLNM_HEADER_BYTES or off_val >= total_bytes:
            raise ValueError(f"Invalid/unaligned section offset: {off_val}")

    vocab: list[str] = []
    char_to_id: dict[str, int] = {}
    for idx in range(vocab_size):
        s_off, s_len = struct.unpack_from("<II", raw, off_vocab_meta + idx * 8)
        tok = raw[off_vocab_strings + s_off : off_vocab_strings + s_off + s_len].decode("utf-8")
        vocab.append(tok)
        char_to_id[tok] = idx

    def read_f32_vec(pos: int, length: int) -> tuple[np.ndarray, int]:
        nbytes = length * 4
        vec = np.frombuffer(raw, dtype="<f4", count=length, offset=pos).astype(np.float32).copy()
        return vec, pos + align8(nbytes)

    def read_i8_matrix(pos: int, rows: int, cols: int) -> tuple[np.ndarray, int]:
        nbytes = rows * cols
        mat_q = np.frombuffer(raw, dtype=np.int8, count=nbytes, offset=pos).reshape(rows, cols)
        pos = pos + align8(nbytes)
        scales, pos = read_f32_vec(pos, rows)
        dequant = mat_q.astype(np.float32) * scales[:, None]
        return dequant, pos

    emb_q = np.frombuffer(raw, dtype=np.int8, count=vocab_size * d_model, offset=off_tok_emb_q).reshape(
        vocab_size, d_model
    )
    emb_s = np.frombuffer(raw, dtype="<f4", count=vocab_size, offset=off_tok_emb_s).astype(np.float32)
    tok_emb = emb_q.astype(np.float32) * emb_s[:, None]

    norm_final = np.frombuffer(raw, dtype="<f4", count=d_model, offset=off_norm_final).astype(np.float32).copy()

    layers: list[dict[str, np.ndarray]] = []
    cursor = off_layers
    for _ in range(n_layers):
        attn_norm, cursor = read_f32_vec(cursor, d_model)
        ffn_norm, cursor = read_f32_vec(cursor, d_model)
        wq, cursor = read_i8_matrix(cursor, d_model, d_model)
        wk, cursor = read_i8_matrix(cursor, d_model, d_model)
        wv, cursor = read_i8_matrix(cursor, d_model, d_model)
        wo, cursor = read_i8_matrix(cursor, d_model, d_model)
        w_gate, cursor = read_i8_matrix(cursor, d_ff, d_model)
        w_up, cursor = read_i8_matrix(cursor, d_ff, d_model)
        w_down, cursor = read_i8_matrix(cursor, d_model, d_ff)
        layers.append(
            {
                "attn_norm": attn_norm,
                "ffn_norm": ffn_norm,
                "wq": wq,
                "wk": wk,
                "wv": wv,
                "wo": wo,
                "w_gate": w_gate,
                "w_up": w_up,
                "w_down": w_down,
            }
        )

    if cursor != total_bytes:
        raise ValueError(f"Layer parsing cursor mismatch: {cursor} != {total_bytes}")

    return {
        "cfg": {
            "flags": flags,
            "vocab_size": vocab_size,
            "d_model": d_model,
            "n_layers": n_layers,
            "n_heads": n_heads,
            "d_ff": d_ff,
            "max_seq_len": max_seq_len,
            "rope_theta": rope_theta,
            "corpus_fingerprint": corpus_fp,
            "rms_eps": rms_eps,
        },
        "vocab": vocab,
        "char_to_id": char_to_id,
        "tok_emb": tok_emb,
        "norm_final": norm_final,
        "layers": layers,
    }


def _rmsnorm_np(x: np.ndarray, weight: np.ndarray, eps: float) -> np.ndarray:
    rms = np.sqrt(np.mean(x * x, axis=-1, keepdims=True) + eps)
    return (x / rms) * weight


def _apply_rope_np(q: np.ndarray, k: np.ndarray, theta: float) -> tuple[np.ndarray, np.ndarray]:
    # q, k: [T, H, D_h]
    seq_len, _, head_dim = q.shape
    half = head_dim // 2
    inv_freq = 1.0 / (theta ** (np.arange(0, half, dtype=np.float32) * 2.0 / head_dim))
    pos = np.arange(seq_len, dtype=np.float32)[:, None]
    freqs = pos * inv_freq[None, :]  # [T, half]
    cos = np.cos(freqs)[:, None, :]
    sin = np.sin(freqs)[:, None, :]

    def rot(z: np.ndarray) -> np.ndarray:
        z_even = z[..., 0::2]
        z_odd = z[..., 1::2]
        out = np.empty_like(z)
        out[..., 0::2] = z_even * cos - z_odd * sin
        out[..., 1::2] = z_even * sin + z_odd * cos
        return out

    return rot(q), rot(k)


def _softmax_np(x: np.ndarray, axis: int = -1) -> np.ndarray:
    m = np.max(x, axis=axis, keepdims=True)
    e = np.exp(x - m)
    return e / np.sum(e, axis=axis, keepdims=True)


def _silu_np(x: np.ndarray) -> np.ndarray:
    return x / (1.0 + np.exp(-np.clip(x, -60.0, 60.0)))


def forward_rlnm0001_numpy(model: dict[str, Any], token_ids: list[int] | np.ndarray) -> np.ndarray:
    """純 NumPy 因果前向傳播，輸入 [T] token IDs，回傳 [T, vocab_size] logits。"""
    cfg = model["cfg"]
    d_model = cfg["d_model"]
    n_heads = cfg["n_heads"]
    head_dim = d_model // n_heads
    eps = cfg["rms_eps"]
    theta = cfg["rope_theta"]
    tok_emb = model["tok_emb"]

    ids = np.asarray(token_ids, dtype=np.int64)
    t_len = ids.shape[0]
    x = tok_emb[ids].copy()  # [T, D]

    causal_mask = np.triu(np.full((t_len, t_len), -1e9, dtype=np.float32), k=1)
    scale = 1.0 / math.sqrt(head_dim)

    for layer in model["layers"]:
        h = _rmsnorm_np(x, layer["attn_norm"], eps)
        q = (h @ layer["wq"].T).reshape(t_len, n_heads, head_dim)
        k = (h @ layer["wk"].T).reshape(t_len, n_heads, head_dim)
        v = (h @ layer["wv"].T).reshape(t_len, n_heads, head_dim)
        q, k = _apply_rope_np(q, k, theta)

        # [H, T, D_h]
        qh = q.transpose(1, 0, 2)
        kh = k.transpose(1, 0, 2)
        vh = v.transpose(1, 0, 2)

        scores = (qh @ kh.transpose(0, 2, 1)) * scale + causal_mask[None, :, :]
        attn = _softmax_np(scores, axis=-1)
        out = (attn @ vh).transpose(1, 0, 2).reshape(t_len, d_model)
        x = x + (out @ layer["wo"].T)

        hf = _rmsnorm_np(x, layer["ffn_norm"], eps)
        gate = _silu_np(hf @ layer["w_gate"].T)
        up = hf @ layer["w_up"].T
        ffn_out = (gate * up) @ layer["w_down"].T
        x = x + ffn_out

    x = _rmsnorm_np(x, model["norm_final"], eps)
    logits = x @ tok_emb.T  # [T, V]
    return logits


def encode_chars(char_to_id: dict[str, int], text: str) -> list[int]:
    return [char_to_id.get(ch, UNK_ID) for ch in text]


def score_candidate_rlnm0001_numpy(
    model: dict[str, Any],
    cand_text: str,
    prefix_text: str = "",
) -> float:
    """計算 log P(cand_text | <bos> + prefix_text) 之自然對數總和。"""
    c2i = model["char_to_id"]
    max_len = int(model["cfg"]["max_seq_len"])
    pref_ids = encode_chars(c2i, prefix_text)
    cand_ids = encode_chars(c2i, cand_text)
    if not cand_ids:
        return 0.0

    # 若超出 max_seq_len，優先保留完整候選與最近前文
    budget = max(1, max_len - 1 - len(cand_ids))
    if len(pref_ids) > budget:
        pref_ids = pref_ids[-budget:]
    seq = [BOS_ID] + pref_ids + cand_ids
    if len(seq) > max_len:
        seq = [BOS_ID] + cand_ids[-(max_len - 1) :]
        cand_start_in_seq = 1
    else:
        cand_start_in_seq = 1 + len(pref_ids)

    logits = forward_rlnm0001_numpy(model, seq[:-1])  # [T-1, V]
    total_lp = 0.0
    for pos in range(cand_start_in_seq - 1, len(seq) - 1):
        target_id = seq[pos + 1]
        row = logits[pos]
        m = float(np.max(row))
        lse = m + math.log(float(np.sum(np.exp(row - m))))
        total_lp += float(row[target_id]) - lse
    return total_lp


# ── 無需 PyTorch 之格式與數學自檢 ────────────────────────────────────────────

def run_self_test_format() -> int:
    print("[self-test] 啟動 RLNM0001 格式與 NumPy 前向推論自檢（無須 PyTorch）...", flush=True)
    rng = np.random.default_rng(20261124)

    vocab = list(SPECIAL_TOKENS) + [chr(0x4E00 + i) for i in range(60)]
    vocab_size = len(vocab)
    cfg = {
        "d_model": 32,
        "n_layers": 2,
        "n_heads": 4,
        "d_ff": 64,
        "max_seq_len": 64,
        "rope_theta": 10000.0,
        "rms_eps": 1e-5,
        "flags": DEFAULT_FLAGS,
    }
    d_model = cfg["d_model"]
    d_ff = cfg["d_ff"]

    tensors: dict[str, np.ndarray] = {
        "tok_emb": (rng.standard_normal((vocab_size, d_model)) * 0.15).astype(np.float32),
        "norm_final": rng.uniform(0.85, 1.15, size=(d_model,)).astype(np.float32),
    }
    for li in range(cfg["n_layers"]):
        tensors[f"layers.{li}.attn_norm"] = rng.uniform(0.85, 1.15, size=(d_model,)).astype(np.float32)
        tensors[f"layers.{li}.ffn_norm"] = rng.uniform(0.85, 1.15, size=(d_model,)).astype(np.float32)
        for nm in ("wq", "wk", "wv", "wo"):
            tensors[f"layers.{li}.{nm}"] = (rng.standard_normal((d_model, d_model)) * 0.12).astype(np.float32)
        tensors[f"layers.{li}.w_gate"] = (rng.standard_normal((d_ff, d_model)) * 0.12).astype(np.float32)
        tensors[f"layers.{li}.w_up"] = (rng.standard_normal((d_ff, d_model)) * 0.12).astype(np.float32)
        tensors[f"layers.{li}.w_down"] = (rng.standard_normal((d_model, d_ff)) * 0.12).astype(np.float32)

    ref_layers = []
    for li in range(cfg["n_layers"]):
        ref_layers.append(
            {
                k: tensors[f"layers.{li}.{k}"]
                for k in ("attn_norm", "ffn_norm", "wq", "wk", "wv", "wo", "w_gate", "w_up", "w_down")
            }
        )
    ref_model = {
        "cfg": cfg,
        "vocab": vocab,
        "char_to_id": {t: i for i, t in enumerate(vocab)},
        "tok_emb": tensors["tok_emb"],
        "norm_final": tensors["norm_final"],
        "layers": ref_layers,
    }

    with tempfile.TemporaryDirectory() as td:
        bin_path = Path(td) / "test_rlnm0001.bin"
        meta = write_rlnm0001(bin_path, vocab, cfg, tensors, corpus_fingerprint=0x123456789ABCDEF0)
        print(f"  [1/4] 寫入 RLNM0001 成功: {meta}", flush=True)
        assert meta["total_bytes"] % 8 == 0

        loaded = load_rlnm0001_numpy(bin_path)
        assert loaded["vocab"] == vocab
        assert loaded["cfg"]["corpus_fingerprint"] == 0x123456789ABCDEF0

        # 1. INT8 反量化誤差驗證
        seq = [BOS_ID, 4, 8, 12, 16, 20, 24]
        logits_fp32 = forward_rlnm0001_numpy(ref_model, seq)
        logits_int8 = forward_rlnm0001_numpy(loaded, seq)
        max_diff = float(np.max(np.abs(logits_fp32 - logits_int8)))
        mean_diff = float(np.mean(np.abs(logits_fp32 - logits_int8)))
        print(
            f"  [2/4] FP32 vs INT8 logits 誤差: max_abs={max_diff:.6f}, mean_abs={mean_diff:.6f}",
            flush=True,
        )
        assert max_diff < 0.05, f"INT8 quantization drift too large: {max_diff}"

        # 2. 因果遮罩 (Causal Mask) 不變量驗證
        seq_mutated = [BOS_ID, 4, 8, 12, 55, 56, 57]
        logits_mut = forward_rlnm0001_numpy(loaded, seq_mutated)
        prefix_drift = float(np.max(np.abs(logits_int8[:4] - logits_mut[:4])))
        print(f"  [3/4] 因果遮罩不變量檢驗（更改 token 4..6 後位置 0..3 最大偏移）: {prefix_drift:.3e}", flush=True)
        assert prefix_drift < 1e-6, f"Causal mask leaked future tokens: {prefix_drift}"

        # 3. 校驗碼損毀偵測 + 前綴分數一致性
        t_a = "".join(vocab[4:10])
        t_b = "".join(vocab[4:7]) + "".join(vocab[15:18])
        lp_a = score_candidate_rlnm0001_numpy(loaded, t_a)
        lp_b = score_candidate_rlnm0001_numpy(loaded, t_b)
        lp_suffix_a = score_candidate_rlnm0001_numpy(loaded, t_a[3:], prefix_text=t_a[:3])
        lp_suffix_b = score_candidate_rlnm0001_numpy(loaded, t_b[3:], prefix_text=t_b[:3])
        diff_delta = abs((lp_a - lp_b) - (lp_suffix_a - lp_suffix_b))
        print(
            f"  [4/4] 前綴共享條件機率差一致性: full_delta={lp_a - lp_b:.6f}, suffix_delta={lp_suffix_a - lp_suffix_b:.6f}, diff={diff_delta:.3e}",
            flush=True,
        )
        assert diff_delta < 1e-4

        corrupted = bytearray(bin_path.read_bytes())
        corrupted[-17] ^= 0xFF
        bad_path = Path(td) / "corrupted.bin"
        bad_path.write_bytes(bytes(corrupted))
        corrupt_caught = False
        try:
            load_rlnm0001_numpy(bad_path)
        except ValueError:
            corrupt_caught = True
        assert corrupt_caught, "Checksum validation failed to detect payload corruption!"

    print("[self-test] 全部斷言通過！RLNM0001 格式與數學不變量 100% 正確。", flush=True)
    return 0


# ── 語料清洗串流（複用 train_ngram_lm.py） ──────────────────────────────────

class _CorpusTextSink:
    """搭配 train_ngram_lm.CorpusIngestor 將清潔台灣正體句段串流寫出為純文字檔。"""

    def __init__(self, out_fp):
        self.out_fp = out_fp
        self.lines_written = 0

    def ingest_clauses(self, clauses: list[str]) -> None:
        for cl in clauses:
            self.out_fp.write(cl)
            self.out_fp.write("\n")
            self.lines_written += 1


def prepare_clean_corpus_txt(
    out_corpus_txt: Path,
    dev_hashes_path: Path,
    target_chars: int = 190_000_000,
    fineweb_timeout_sec: int = 900,
    max_trad_zh_docs: int = 500_000,
) -> dict[str, Any]:
    scripts_dir = Path(__file__).resolve().parent
    if str(scripts_dir) not in sys.path:
        sys.path.insert(0, str(scripts_dir))
    import train_ngram_lm as tnl  # noqa: E402

    dev_hashes = tnl.load_dev_hashes(dev_hashes_path)
    purity = tnl.TaiwanPurityFilter(dev_hashes)
    out_corpus_txt.parent.mkdir(parents=True, exist_ok=True)

    with open(out_corpus_txt, "w", encoding="utf-8") as f:
        sink = _CorpusTextSink(f)
        ingestor = tnl.CorpusIngestor(purity, sink, target_chars, fineweb_timeout_sec)

        ingestor.run_fineweb_tw(cap=70_000_000)
        ingestor.run_common_voice_tw(cap=5_000_000)
        ingestor.run_auto_repo_source("tw_chinese_chat", "lianghsun/tw-chinese-chat", "MIT", cap=15_000_000)
        ingestor.run_auto_repo_source("coco_caption_zh_tw_val", "lianghsun/coco-caption-zh_tw-val", "CC-BY-4.0", cap=3_000_000)
        ingestor.run_ivod(cap=20_000_000)
        ingestor.run_ly_interpellation(cap=4_000_000)
        ingestor.run_ly_bill(cap=2_000_000)
        ingestor.run_parquet_source(
            "tw_reasoning_instruct_50k",
            "twinkle-ai/tw-reasoning-instruct-50k",
            "data/datasets.parquet",
            "MIT",
            cap=20_000_000,
            preferred_cols=("input", "output", "think"),
        )
        ingestor.run_parquet_source(
            "tw_function_call_reasoning_10k",
            "twinkle-ai/tw-function-call-reasoning-10k",
            "data/train-00000-of-00001.parquet",
            "CC-BY-4.0",
            cap=2_500_000,
            preferred_cols=("query_zhtw", "think"),
        )
        ingestor.run_parquet_source(
            "tw_math_reasoning_2k",
            "twinkle-ai/tw-math-reasoning-2k",
            "data/train-00000-of-00001.parquet",
            "MIT",
            cap=1_500_000,
        )
        ingestor.run_jsonl_source(
            "tw_processed_law_article",
            "lianghsun/tw-processed-law-article",
            "datasets.jsonl",
            "CC-BY-4.0",
            cap=3_500_000,
        )
        ingestor.run_jsonl_source(
            "tw_processed_law_ctx",
            "lianghsun/tw-processed-law-ctx",
            "train.jsonl",
            "Apache-2.0",
            cap=2_500_000,
        )
        ingestor.run_auto_repo_source(
            "traditional_chinese_odc",
            "agentlans/traditional-chinese",
            "ODC-BY",
            cap=35_000_000,
            max_docs=max_trad_zh_docs,
        )

    print("[corpus] 各語料來源清潔字數統計彙總:", flush=True)
    for src_key, src_info in ingestor.source_stats.items():
        print(
            f"  [corpus summary] {src_key}: status={src_info.get('status')} "
            f"docs={src_info.get('docs_scanned', 0):,} clean_chars={src_info.get('clean_chars', 0):,}",
            flush=True,
        )

    return {
        "corpus_txt": str(out_corpus_txt),
        "total_clean_chars": ingestor.total_clean_chars,
        "lines": sink.lines_written,
        "sources": ingestor.source_stats,
    }


def build_char_vocab(
    corpus_txt: Path,
    max_vocab_size: int = 7200,
    lexicon_path: Path | None = None,
) -> list[str]:
    counts: Counter[str] = Counter()
    with open(corpus_txt, "r", encoding="utf-8") as f:
        for line in f:
            s = line.strip()
            if s:
                counts.update(s)

    # 先補足核心詞庫與常用中文標點確保日常繁體字皆在詞表內
    seed_chars: list[str] = list(COMMON_PUNCT_CHARS)
    if lexicon_path and lexicon_path.is_file():
        with open(lexicon_path, "r", encoding="utf-8") as lf:
            for line in lf:
                parts = line.rstrip("\r\n").split("\t")
                if parts and len(parts[0]) == 1:
                    seed_chars.append(parts[0])

    vocab: list[str] = list(SPECIAL_TOKENS)
    seen = set(vocab)
    for ch, _ in counts.most_common():
        if len(vocab) >= max_vocab_size:
            break
        if ch not in seen and not ch.isspace():
            vocab.append(ch)
            seen.add(ch)

    for ch in seed_chars:
        if len(vocab) >= max_vocab_size:
            break
        if ch not in seen and not ch.isspace():
            vocab.append(ch)
            seen.add(ch)

    return vocab


# ── PyTorch 延遲載入模型與訓練迴圈 ────────────────────────────────────────────

def _build_torch_model_class():
    import torch
    import torch.nn as nn
    import torch.nn.functional as F

    class RMSNorm(nn.Module):
        def __init__(self, dim: int, eps: float = 1e-5):
            super().__init__()
            self.eps = eps
            self.weight = nn.Parameter(torch.ones(dim))

        def forward(self, x: torch.Tensor) -> torch.Tensor:
            rms = torch.rsqrt(x.pow(2).mean(dim=-1, keepdim=True) + self.eps)
            return (x * rms) * self.weight

    def apply_rope(q: torch.Tensor, k: torch.Tensor, rope_theta: float) -> tuple[torch.Tensor, torch.Tensor]:
        # q, k: [B, H, T, D_h]
        seq_len = q.shape[2]
        head_dim = q.shape[3]
        half = head_dim // 2
        inv_freq = 1.0 / (
            rope_theta ** (torch.arange(0, half, device=q.device, dtype=torch.float32) * 2.0 / head_dim)
        )
        pos = torch.arange(seq_len, device=q.device, dtype=torch.float32).unsqueeze(1)
        freqs = pos * inv_freq.unsqueeze(0)  # [T, half]
        cos = freqs.cos().unsqueeze(0).unsqueeze(0).to(q.dtype)
        sin = freqs.sin().unsqueeze(0).unsqueeze(0).to(q.dtype)

        def rot(z: torch.Tensor) -> torch.Tensor:
            ze = z[..., 0::2]
            zo = z[..., 1::2]
            out = torch.empty_like(z)
            out[..., 0::2] = ze * cos - zo * sin
            out[..., 1::2] = ze * sin + zo * cos
            return out

        return rot(q), rot(k)

    class CausalSelfAttention(nn.Module):
        def __init__(self, d_model: int, n_heads: int, rope_theta: float):
            super().__init__()
            self.n_heads = n_heads
            self.head_dim = d_model // n_heads
            self.rope_theta = rope_theta
            self.wq = nn.Linear(d_model, d_model, bias=False)
            self.wk = nn.Linear(d_model, d_model, bias=False)
            self.wv = nn.Linear(d_model, d_model, bias=False)
            self.wo = nn.Linear(d_model, d_model, bias=False)

        def forward(self, x: torch.Tensor) -> torch.Tensor:
            bsz, seq_len, d_model = x.shape
            q = self.wq(x).view(bsz, seq_len, self.n_heads, self.head_dim).transpose(1, 2)
            k = self.wk(x).view(bsz, seq_len, self.n_heads, self.head_dim).transpose(1, 2)
            v = self.wv(x).view(bsz, seq_len, self.n_heads, self.head_dim).transpose(1, 2)
            q, k = apply_rope(q, k, self.rope_theta)
            out = F.scaled_dot_product_attention(q, k, v, is_causal=True)
            out = out.transpose(1, 2).contiguous().view(bsz, seq_len, d_model)
            return self.wo(out)

    class SwiGLU(nn.Module):
        def __init__(self, d_model: int, d_ff: int):
            super().__init__()
            self.w_gate = nn.Linear(d_model, d_ff, bias=False)
            self.w_up = nn.Linear(d_model, d_ff, bias=False)
            self.w_down = nn.Linear(d_ff, d_model, bias=False)

        def forward(self, x: torch.Tensor) -> torch.Tensor:
            return self.w_down(F.silu(self.w_gate(x)) * self.w_up(x))

    class Block(nn.Module):
        def __init__(self, d_model: int, n_heads: int, d_ff: int, rope_theta: float, rms_eps: float):
            super().__init__()
            self.attn_norm = RMSNorm(d_model, eps=rms_eps)
            self.attn = CausalSelfAttention(d_model, n_heads, rope_theta)
            self.ffn_norm = RMSNorm(d_model, eps=rms_eps)
            self.ffn = SwiGLU(d_model, d_ff)

        def forward(self, x: torch.Tensor) -> torch.Tensor:
            x = x + self.attn(self.attn_norm(x))
            x = x + self.ffn(self.ffn_norm(x))
            return x

    class RuhICharLM(nn.Module):
        def __init__(
            self,
            vocab_size: int,
            d_model: int = 512,
            n_layers: int = 6,
            n_heads: int = 8,
            d_ff: int = 1368,
            max_seq_len: int = 256,
            rope_theta: float = 10000.0,
            rms_eps: float = 1e-5,
        ):
            super().__init__()
            self.cfg = {
                "vocab_size": vocab_size,
                "d_model": d_model,
                "n_layers": n_layers,
                "n_heads": n_heads,
                "d_ff": d_ff,
                "max_seq_len": max_seq_len,
                "rope_theta": rope_theta,
                "rms_eps": rms_eps,
                "flags": DEFAULT_FLAGS,
            }
            self.tok_emb = nn.Embedding(vocab_size, d_model)
            self.layers = nn.ModuleList(
                [Block(d_model, n_heads, d_ff, rope_theta, rms_eps) for _ in range(n_layers)]
            )
            self.norm_final = RMSNorm(d_model, eps=rms_eps)
            self.apply(self._init_weights)

        @staticmethod
        def _init_weights(mod: nn.Module) -> None:
            if isinstance(mod, nn.Linear):
                nn.init.normal_(mod.weight, mean=0.0, std=0.02)
            elif isinstance(mod, nn.Embedding):
                nn.init.normal_(mod.weight, mean=0.0, std=0.02)

        def forward(self, idx: torch.Tensor) -> torch.Tensor:
            x = self.tok_emb(idx)
            for layer in self.layers:
                x = layer(x)
            x = self.norm_final(x)
            return F.linear(x, self.tok_emb.weight)

        def export_numpy_tensors(self) -> dict[str, np.ndarray]:
            out: dict[str, np.ndarray] = {
                "tok_emb": self.tok_emb.weight.detach().float().cpu().numpy(),
                "norm_final": self.norm_final.weight.detach().float().cpu().numpy(),
            }
            for li, blk in enumerate(self.layers):
                out[f"layers.{li}.attn_norm"] = blk.attn_norm.weight.detach().float().cpu().numpy()
                out[f"layers.{li}.ffn_norm"] = blk.ffn_norm.weight.detach().float().cpu().numpy()
                out[f"layers.{li}.wq"] = blk.attn.wq.weight.detach().float().cpu().numpy()
                out[f"layers.{li}.wk"] = blk.attn.wk.weight.detach().float().cpu().numpy()
                out[f"layers.{li}.wv"] = blk.attn.wv.weight.detach().float().cpu().numpy()
                out[f"layers.{li}.wo"] = blk.attn.wo.weight.detach().float().cpu().numpy()
                out[f"layers.{li}.w_gate"] = blk.ffn.w_gate.weight.detach().float().cpu().numpy()
                out[f"layers.{li}.w_up"] = blk.ffn.w_up.weight.detach().float().cpu().numpy()
                out[f"layers.{li}.w_down"] = blk.ffn.w_down.weight.detach().float().cpu().numpy()
            return out

    return RuhICharLM


def encode_corpus_to_uint16_bin(
    corpus_txt: Path,
    vocab: list[str],
    out_bin: Path,
    val_fraction: float = 0.01,
    seed: int = 20261124,
    block_size: int = 8,
) -> dict[str, Any]:
    """將清洗純文字語料按微區塊洗牌後打包為 train/val uint16 token 陣列與 <bos> 句界索引。"""
    c2i = {ch: i for i, ch in enumerate(vocab)}
    lines: list[str] = []
    with open(corpus_txt, "r", encoding="utf-8") as fin:
        for line in fin:
            s = line.strip()
            if s:
                lines.append(s)

    blocks = [lines[i : i + block_size] for i in range(0, len(lines), block_size)]
    rng = np.random.default_rng(seed)
    perm = rng.permutation(len(blocks))

    train_path = out_bin.with_suffix(".train.u16")
    val_path = out_bin.with_suffix(".val.u16")
    train_bos_path = out_bin.with_suffix(".train_bos.i64")
    train_lens_path = out_bin.with_suffix(".train_lens.u16")
    val_bos_path = out_bin.with_suffix(".val_bos.i64")
    val_lens_path = out_bin.with_suffix(".val_lens.u16")

    val_mod = max(2, int(round(1.0 / max(val_fraction, 1e-4))))
    buf_tr: list[int] = []
    buf_va: list[int] = []
    bos_tr: list[int] = []
    lens_tr: list[int] = []
    bos_va: list[int] = []
    lens_va: list[int] = []
    off_tr = 0
    off_va = 0
    total_tokens = 0

    with (
        open(train_path, "wb") as ftr,
        open(val_path, "wb") as fva,
        open(train_bos_path, "wb") as fbtr,
        open(train_lens_path, "wb") as fltr,
        open(val_bos_path, "wb") as fbva,
        open(val_lens_path, "wb") as flva,
    ):
        for out_block_idx, orig_idx in enumerate(perm):
            blk = blocks[int(orig_idx)]
            is_val = (out_block_idx % val_mod) == 0
            dst_tok = buf_va if is_val else buf_tr
            dst_bos = bos_va if is_val else bos_tr
            dst_lens = lens_va if is_val else lens_tr

            for s in blk:
                toks = [BOS_ID] + [c2i.get(ch, UNK_ID) for ch in s] + [EOS_ID]
                tlen = len(toks)
                total_tokens += tlen
                if is_val:
                    dst_bos.append(off_va)
                    off_va += tlen
                else:
                    dst_bos.append(off_tr)
                    off_tr += tlen
                dst_lens.append(tlen)
                dst_tok.extend(toks)

            if len(buf_tr) >= 1_000_000:
                np.asarray(buf_tr, dtype=np.uint16).tofile(ftr)
                buf_tr.clear()
                np.asarray(bos_tr, dtype=np.int64).tofile(fbtr)
                bos_tr.clear()
                np.asarray(lens_tr, dtype=np.uint16).tofile(fltr)
                lens_tr.clear()
            if len(buf_va) >= 200_000:
                np.asarray(buf_va, dtype=np.uint16).tofile(fva)
                buf_va.clear()
                np.asarray(bos_va, dtype=np.int64).tofile(fbva)
                bos_va.clear()
                np.asarray(lens_va, dtype=np.uint16).tofile(flva)
                lens_va.clear()

        if buf_tr:
            np.asarray(buf_tr, dtype=np.uint16).tofile(ftr)
            np.asarray(bos_tr, dtype=np.int64).tofile(fbtr)
            np.asarray(lens_tr, dtype=np.uint16).tofile(fltr)
        if buf_va:
            np.asarray(buf_va, dtype=np.uint16).tofile(fva)
            np.asarray(bos_va, dtype=np.int64).tofile(fbva)
            np.asarray(lens_va, dtype=np.uint16).tofile(flva)

    return {
        "train_path": train_path,
        "val_path": val_path,
        "train_bos_path": train_bos_path,
        "train_lens_path": train_lens_path,
        "val_bos_path": val_bos_path,
        "val_lens_path": val_lens_path,
        "total_tokens": total_tokens,
        "total_clauses": len(lines),
        "block_size": block_size,
    }


def build_homophone_confusion_map(
    lexicon_path: Path, vocab: list[str]
) -> tuple[dict[str, tuple[list[str], np.ndarray]], dict[str, tuple[list[str], np.ndarray]]]:
    """自核心詞庫依完全相同注音讀音建立 1 字／2 字高頻同音競爭替換表（供無評估集洩漏之對比排序微調）。"""
    special_set = set(SPECIAL_TOKENS)
    vocab_set = {tok for tok in vocab if tok not in special_set and len(tok) == 1}
    by_len_bpm: dict[tuple[int, str], dict[str, int]] = {}
    best_reading: dict[tuple[int, str], tuple[str, int]] = {}

    with open(lexicon_path, "r", encoding="utf-8") as f:
        for line in f:
            s = line.strip()
            if not s or s.startswith("#"):
                continue
            parts = s.split("\t")
            if len(parts) < 3:
                continue
            word, bpm = parts[0].strip(), " ".join(parts[1].strip().split())
            wlen = len(word)
            if wlen not in (1, 2) or not bpm:
                continue
            if not all(ch in vocab_set for ch in word):
                continue
            try:
                wt = int(parts[2])
            except ValueError:
                continue
            min_wt = 20 if wlen == 1 else 50
            if wt < min_wt:
                continue
            key = (wlen, bpm)
            bucket = by_len_bpm.setdefault(key, {})
            if wt > bucket.get(word, 0):
                bucket[word] = wt
            prev = best_reading.get((wlen, word))
            if prev is None or wt > prev[1]:
                best_reading[(wlen, word)] = (bpm, wt)

    pruned_buckets: dict[tuple[int, str], list[tuple[str, int]]] = {}
    for (wlen, bpm), entries in by_len_bpm.items():
        cap = 8 if wlen == 1 else 6
        top = sorted(entries.items(), key=lambda kv: (-kv[1], kv[0]))[:cap]
        if len(top) >= 2:
            pruned_buckets[(wlen, bpm)] = top

    conf1: dict[str, tuple[list[str], np.ndarray]] = {}
    conf2: dict[str, tuple[list[str], np.ndarray]] = {}
    for (wlen, word), (primary_bpm, _) in best_reading.items():
        grp = pruned_buckets.get((wlen, primary_bpm))
        if not grp:
            continue
        alts = [(alt, wt) for alt, wt in grp if alt != word]
        if not alts:
            continue
        words = [a for a, _ in alts]
        raw_p = np.array([max(1.0, float(wt)) ** 0.7 for _, wt in alts], dtype=np.float64)
        probs = raw_p / raw_p.sum()
        if wlen == 1:
            conf1[word] = (words, probs)
        else:
            conf2[word] = (words, probs)
    return conf1, conf2


def corrupt_clause_with_homophones(
    text: str,
    conf1: dict[str, tuple[list[str], np.ndarray]],
    conf2: dict[str, tuple[list[str], np.ndarray]],
    rng: np.random.Generator,
) -> str | None:
    """於保持句長不變前提下隨機挑選 1–2 處互不重疊片段進行同注音替換，生成逼真負樣本。"""
    spans: list[tuple[int, int, list[str], np.ndarray]] = []
    i = 0
    n = len(text)
    while i < n:
        if i + 2 <= n:
            w2 = text[i : i + 2]
            if w2 in conf2:
                alts, probs = conf2[w2]
                spans.append((i, i + 2, alts, probs))
                i += 2
                continue
        w1 = text[i]
        if w1 in conf1:
            alts, probs = conf1[w1]
            spans.append((i, i + 1, alts, probs))
        i += 1

    if not spans:
        return None
    k_pick = 2 if (len(spans) >= 2 and rng.random() < 0.25) else 1
    chosen_idxs = sorted(rng.choice(len(spans), size=k_pick, replace=False).tolist())
    chars = list(text)
    changed = False
    for si in chosen_idxs:
        s0, s1, alts, probs = spans[si]
        rep = str(rng.choice(alts, p=probs))
        if len(rep) == (s1 - s0) and "".join(chars[s0:s1]) != rep:
            chars[s0:s1] = list(rep)
            changed = True
    if not changed:
        return None
    neg = "".join(chars)
    return neg if neg != text else None


def run_contrastive_finetune(
    model,
    vocab: list[str],
    train_mm: np.memmap,
    train_bos: np.memmap,
    train_lens: np.memmap,
    lexicon_path: Path,
    args: argparse.Namespace,
    device,
    amp_dtype,
    use_amp: bool,
    block_size: int,
) -> None:
    """Stage D3-2：僅利用去重訓練集句段＋核心詞庫同音競爭詞執行 CE + Softplus Margin 對比微調（零評估集接觸）。"""
    import torch
    import torch.nn.functional as F

    conf1, conf2 = build_homophone_confusion_map(lexicon_path, vocab)
    print(
        f"[contrastive] 同音競爭表載入完成: 1字={len(conf1):,}, 2字={len(conf2):,} | "
        f"steps={args.contrastive_steps}, lr={args.contrastive_lr:.1e}, margin={args.contrastive_margin}, alpha={args.contrastive_alpha}",
        flush=True,
    )
    if len(conf1) < 100:
        raise RuntimeError("同音競爭詞表過小，中止對比微調以免模型崩壞。")

    c2i = {c: i for i, c in enumerate(vocab)}
    rng = np.random.default_rng(args.seed + 31337)
    n_clauses = len(train_bos)
    max_seq_len = int(args.max_seq_len)
    bsz = 64

    opt = torch.optim.AdamW(
        model.parameters(),
        lr=args.contrastive_lr,
        betas=(0.9, 0.95),
        weight_decay=0.05,
    )
    use_scaler = use_amp and (amp_dtype == torch.float16)
    scaler = torch.amp.GradScaler("cuda", enabled=use_scaler)

    def sample_contrastive_pair_batch() -> tuple[torch.Tensor, torch.Tensor, torch.Tensor, torch.Tensor]:
        gold_seqs: list[list[int]] = []
        neg_seqs: list[list[int]] = []
        pref_lens: list[int] = []
        attempts = 0
        while len(gold_seqs) < bsz and attempts < bsz * 40:
            attempts += 1
            idx = int(rng.integers(0, n_clauses))
            b0 = int(train_bos[idx])
            l0 = int(train_lens[idx])
            if l0 < 8 or l0 > 44:
                continue
            toks_raw = train_mm[b0 + 1 : b0 + l0 - 1]
            if np.any(toks_raw == UNK_ID) or np.any(toks_raw < 4):
                continue
            text = "".join(vocab[int(t)] for t in toks_raw)
            if not (6 <= len(text) <= 40):
                continue
            neg_text = corrupt_clause_with_homophones(text, conf1, conf2, rng)
            if neg_text is None or len(neg_text) != len(text):
                continue

            prefix_toks: list[int] = [BOS_ID]
            if rng.random() < 0.50 and idx > 0 and ((idx - 1) // block_size) == (idx // block_size):
                bp = int(train_bos[idx - 1])
                lp = int(train_lens[idx - 1])
                if lp > 3:
                    p_slice = train_mm[bp + 1 : bp + lp - 1].tolist()
                    if len(p_slice) > 48:
                        p_slice = p_slice[-48:]
                    if len(prefix_toks) + len(p_slice) + len(text) <= max_seq_len:
                        prefix_toks.extend(int(t) for t in p_slice)

            g_ids = [c2i.get(ch, UNK_ID) for ch in text]
            n_ids = [c2i.get(ch, UNK_ID) for ch in neg_text]
            if UNK_ID in g_ids or UNK_ID in n_ids:
                continue
            gold_seqs.append(prefix_toks + g_ids)
            neg_seqs.append(prefix_toks + n_ids)
            pref_lens.append(len(prefix_toks))

        if not gold_seqs:
            raise RuntimeError("無法合成任何同音對比微調批次。")
        t_max = max(len(s) for s in gold_seqs) - 1
        cur_b = len(gold_seqs)
        inp_g = torch.full((cur_b, t_max), PAD_ID, dtype=torch.long)
        tgt_g = torch.full((cur_b, t_max), 0, dtype=torch.long)
        inp_n = torch.full((cur_b, t_max), PAD_ID, dtype=torch.long)
        tgt_n = torch.full((cur_b, t_max), 0, dtype=torch.long)
        mask = torch.zeros((cur_b, t_max), dtype=torch.float32)

        for bi in range(cur_b):
            gs = gold_seqs[bi]
            ns = neg_seqs[bi]
            plen = pref_lens[bi]
            l_eff = len(gs) - 1
            inp_g[bi, :l_eff] = torch.tensor(gs[:-1], dtype=torch.long)
            tgt_g[bi, :l_eff] = torch.tensor(gs[1:], dtype=torch.long)
            inp_n[bi, :l_eff] = torch.tensor(ns[:-1], dtype=torch.long)
            tgt_n[bi, :l_eff] = torch.tensor(ns[1:], dtype=torch.long)
            mask[bi, (plen - 1) : l_eff] = 1.0

        return (
            torch.cat([inp_g, inp_n], dim=0).to(device),
            tgt_g.to(device),
            tgt_n.to(device),
            mask.to(device),
        )

    t_c0 = time.monotonic()
    for cstep in range(args.contrastive_steps):
        prog = cstep / max(1, args.contrastive_steps)
        lr_cur = args.contrastive_lr * (0.1 + 0.9 * 0.5 * (1.0 + math.cos(math.pi * prog)))
        for pg in opt.param_groups:
            pg["lr"] = lr_cur

        model.train()
        opt.zero_grad(set_to_none=True)
        inp_both, tgt_g, tgt_n, mask = sample_contrastive_pair_batch()
        cur_b = tgt_g.shape[0]
        with torch.amp.autocast(device_type=device.type, dtype=amp_dtype, enabled=use_amp):
            logits = model(inp_both)
            log_probs = F.log_softmax(logits.float(), dim=-1)
            lp_g_tok = log_probs[:cur_b].gather(-1, tgt_g.unsqueeze(-1)).squeeze(-1)
            lp_n_tok = log_probs[cur_b:].gather(-1, tgt_n.unsqueeze(-1)).squeeze(-1)
            lp_g = (lp_g_tok * mask).sum(dim=-1)
            lp_n = (lp_n_tok * mask).sum(dim=-1)
            ce_loss = -(lp_g_tok * mask).sum() / mask.sum().clamp(min=1.0)
            diff = lp_g - lp_n
            rank_loss = F.softplus(args.contrastive_margin - diff).mean()
            loss = ce_loss + args.contrastive_alpha * rank_loss

        if use_scaler:
            scaler.scale(loss).backward()
            scaler.unscale_(opt)
        else:
            loss.backward()
        torch.nn.utils.clip_grad_norm_(model.parameters(), 1.0)
        if use_scaler:
            scaler.step(opt)
            scaler.update()
        else:
            opt.step()

        if (cstep + 1) % 200 == 0 or cstep == 0 or (cstep + 1) == args.contrastive_steps:
            pair_acc = float((diff > 0).float().mean().item())
            mean_gap = float(diff.mean().item())
            dt = time.monotonic() - t_c0
            print(
                f"[contrast {cstep + 1:4d}/{args.contrastive_steps}] ce={float(ce_loss):.4f} "
                f"rank={float(rank_loss):.4f} pair_acc={pair_acc * 100:.1f}% gap={mean_gap:+.2f} "
                f"lr={lr_cur:.2e} ({dt:.1f}s)",
                flush=True,
            )


def run_torch_training(args: argparse.Namespace) -> int:
    import torch
    import torch.nn.functional as F

    RuhICharLM = _build_torch_model_class()
    device = torch.device("cuda" if torch.cuda.is_available() else "cpu")
    if device.type == "cuda":
        torch.set_float32_matmul_precision("high")
    use_bf16 = (
        device.type == "cuda"
        and (args.bf16 or torch.cuda.get_device_capability(0)[0] >= 8)
        and torch.cuda.is_bf16_supported()
    )
    use_amp = device.type == "cuda"
    amp_dtype = torch.bfloat16 if use_bf16 else torch.float16
    use_scaler = use_amp and not use_bf16
    scaler = torch.amp.GradScaler("cuda", enabled=use_scaler)
    print(f"[train] device={device}, use_amp={use_amp}, amp_dtype={amp_dtype}, use_scaler={use_scaler}, cold_start_ratio={args.cold_start_ratio:.2f}", flush=True)

    corpus_txt = Path(args.corpus_txt)
    if not corpus_txt.is_file():
        if not args.dev_hashes:
            print("[錯誤] 找不到 --corpus-txt 且未指定 --dev-hashes 以串流準備語料", file=sys.stderr)
            return 2
        prep = prepare_clean_corpus_txt(
            corpus_txt,
            Path(args.dev_hashes),
            target_chars=args.target_chars,
            fineweb_timeout_sec=args.fineweb_timeout_sec,
            max_trad_zh_docs=args.max_trad_zh_docs,
        )
        print(f"[corpus] 準備完成: clean_chars={prep['total_clean_chars']:,}, lines={prep['lines']:,}", flush=True)

    lex_path = Path(args.lexicon) if args.lexicon else None
    vocab = build_char_vocab(corpus_txt, max_vocab_size=args.vocab_size, lexicon_path=lex_path)
    print(f"[vocab] 建置詞表大小: {len(vocab)}", flush=True)

    work_dir = Path(args.work_dir)
    work_dir.mkdir(parents=True, exist_ok=True)
    enc = encode_corpus_to_uint16_bin(corpus_txt, vocab, work_dir / "tokens", seed=args.seed)
    total_toks = enc["total_tokens"]
    block_size = enc["block_size"]
    train_mm = np.memmap(enc["train_path"], dtype=np.uint16, mode="r")
    val_mm = np.memmap(enc["val_path"], dtype=np.uint16, mode="r")
    train_bos = np.memmap(enc["train_bos_path"], dtype=np.int64, mode="r")
    train_lens = np.memmap(enc["train_lens_path"], dtype=np.uint16, mode="r")
    val_bos = np.memmap(enc["val_bos_path"], dtype=np.int64, mode="r")
    val_lens = np.memmap(enc["val_lens_path"], dtype=np.uint16, mode="r")
    print(
        f"[tokens] total={total_toks:,} | clauses={enc['total_clauses']:,} | train={len(train_mm):,} ({len(train_bos):,} clauses) | val={len(val_mm):,} ({len(val_bos):,} clauses)",
        flush=True,
    )

    model = RuhICharLM(
        vocab_size=len(vocab),
        d_model=args.d_model,
        n_layers=args.n_layers,
        n_heads=args.n_heads,
        d_ff=args.d_ff,
        max_seq_len=args.max_seq_len,
    ).to(device)
    n_params = sum(p.numel() for p in model.parameters())
    print(f"[model] RuhI-CharLM-v1 參數總數: {n_params:,} ({n_params / 1e6:.2f}M)", flush=True)

    optimizer = torch.optim.AdamW(
        model.parameters(),
        lr=args.lr,
        betas=(0.9, 0.95),
        weight_decay=args.weight_decay,
    )

    start_step = 0
    ckpt_path = Path(args.checkpoint) if args.checkpoint else (work_dir / "checkpoint.pt")
    if args.resume and ckpt_path.is_file():
        ckpt = torch.load(ckpt_path, map_location=device, weights_only=False)
        model.load_state_dict(ckpt["model"])
        optimizer.load_state_dict(ckpt["optimizer"])
        start_step = int(ckpt["step"])
        print(f"[resume] 從 step={start_step} 恢復訓練", flush=True)

    rng = np.random.default_rng(args.seed)
    seq_len = args.max_seq_len

    train_valid_bos = np.asarray(train_bos[train_bos <= (len(train_mm) - seq_len - 1)], dtype=np.int64)
    val_valid_bos = np.asarray(val_bos[val_bos <= (len(val_mm) - seq_len - 1)], dtype=np.int64)

    def sample_packed_batch(mm: np.memmap, valid_bos: np.ndarray, bsz: int) -> tuple[torch.Tensor, torch.Tensor]:
        if len(valid_bos) > 0:
            sel = rng.integers(0, len(valid_bos), size=bsz)
            starts = valid_bos[sel]
        else:
            starts = rng.integers(0, max(1, len(mm) - seq_len - 1), size=bsz)
        x_np = np.stack([mm[s : s + seq_len] for s in starts]).astype(np.int64)
        y_np = np.stack([mm[s + 1 : s + 1 + seq_len] for s in starts]).astype(np.int64)
        return torch.from_numpy(x_np).to(device), torch.from_numpy(y_np).to(device)

    def sample_cold_batch(
        mm: np.memmap, bos_arr: np.memmap, lens_arr: np.memmap, bsz: int
    ) -> tuple[torch.Tensor, torch.Tensor]:
        n_clauses = len(bos_arr)
        idxs = rng.integers(0, n_clauses, size=bsz)
        seqs: list[np.ndarray] = []
        max_t = 2
        half_bsz = bsz // 2
        for bi, raw_idx in enumerate(idxs):
            idx = int(raw_idx)
            b0 = int(bos_arr[idx])
            l0 = int(lens_arr[idx])
            # 前半批次：單句冷啟動 [BOS] + clause + [EOS]；後半批次：雙句前綴對齊 [BOS] + c1 + c2 + [EOS]
            if bi >= half_bsz and (idx + 1) < n_clauses and (idx // block_size) == ((idx + 1) // block_size):
                b1 = int(bos_arr[idx + 1])
                l1 = int(lens_arr[idx + 1])
                if l0 + l1 - 2 <= seq_len + 1:
                    c1 = mm[b0 : b0 + l0 - 1]
                    c2 = mm[b1 + 1 : b1 + l1]
                    seq = np.concatenate([c1, c2])
                else:
                    seq = mm[b0 : b0 + l0]
            else:
                seq = mm[b0 : b0 + l0]
            if len(seq) > seq_len + 1:
                seq = seq[: seq_len + 1]
            seqs.append(seq)
            if len(seq) - 1 > max_t:
                max_t = len(seq) - 1

        max_t = min(max_t, seq_len)
        x_np = np.full((bsz, max_t), PAD_ID, dtype=np.int64)
        y_np = np.full((bsz, max_t), -100, dtype=np.int64)
        for bi, seq in enumerate(seqs):
            eff = min(len(seq) - 1, max_t)
            if eff > 0:
                x_np[bi, :eff] = seq[:eff]
                y_np[bi, :eff] = seq[1 : 1 + eff]
        return torch.from_numpy(x_np).to(device), torch.from_numpy(y_np).to(device)

    def lr_for_step(step: int) -> float:
        if step < args.warmup_steps:
            return args.lr * (step + 1) / max(1, args.warmup_steps)
        progress = (step - args.warmup_steps) / max(1, args.max_steps - args.warmup_steps)
        progress = min(max(progress, 0.0), 1.0)
        min_lr = args.lr * 0.1
        return min_lr + 0.5 * (args.lr - min_lr) * (1.0 + math.cos(math.pi * progress))

    t0 = time.monotonic()
    for step in range(start_step, args.max_steps):
        lr_cur = lr_for_step(step)
        for pg in optimizer.param_groups:
            pg["lr"] = lr_cur

        model.train()
        optimizer.zero_grad(set_to_none=True)
        loss_accum = 0.0
        for _ in range(args.grad_accum):
            if args.cold_start_ratio > 0.0 and rng.random() < args.cold_start_ratio:
                xb, yb = sample_cold_batch(train_mm, train_bos, train_lens, args.batch_size)
            else:
                xb, yb = sample_packed_batch(train_mm, train_valid_bos, args.batch_size)
            with torch.amp.autocast(device_type=device.type, dtype=amp_dtype, enabled=use_amp):
                logits = model(xb)
                loss = F.cross_entropy(logits.view(-1, len(vocab)), yb.view(-1), ignore_index=-100)
                loss_scaled = loss / args.grad_accum
            if use_scaler:
                scaler.scale(loss_scaled).backward()
            else:
                loss_scaled.backward()
            loss_accum += float(loss.detach()) / args.grad_accum

        if use_scaler:
            scaler.unscale_(optimizer)
        torch.nn.utils.clip_grad_norm_(model.parameters(), 1.0)
        if use_scaler:
            scaler.step(optimizer)
            scaler.update()
        else:
            optimizer.step()

        if (step + 1) % args.log_every == 0 or step == start_step:
            dt = time.monotonic() - t0
            print(f"[step {step + 1:6d}/{args.max_steps}] loss={loss_accum:.4f} lr={lr_cur:.2e} ({dt:.1f}s)", flush=True)

        if (step + 1) % args.eval_every == 0 or (step + 1) == args.max_steps:
            model.eval()
            val_packed = []
            val_cold = []
            with torch.no_grad():
                for _ in range(16):
                    xv, yv = sample_packed_batch(val_mm, val_valid_bos, args.batch_size)
                    with torch.amp.autocast(device_type=device.type, dtype=amp_dtype, enabled=use_amp):
                        vl = F.cross_entropy(model(xv).view(-1, len(vocab)), yv.view(-1), ignore_index=-100)
                    val_packed.append(float(vl))
                    xc, yc = sample_cold_batch(val_mm, val_bos, val_lens, args.batch_size)
                    with torch.amp.autocast(device_type=device.type, dtype=amp_dtype, enabled=use_amp):
                        vc = F.cross_entropy(model(xc).view(-1, len(vocab)), yc.view(-1), ignore_index=-100)
                    val_cold.append(float(vc))
            mean_packed = sum(val_packed) / len(val_packed)
            mean_cold = sum(val_cold) / len(val_cold)
            print(
                f"[val step {step + 1:6d}] val_packed_loss={mean_packed:.4f} packed_ppl={math.exp(min(mean_packed, 20.0)):.2f} "
                f"| val_cold_loss={mean_cold:.4f} cold_ppl={math.exp(min(mean_cold, 20.0)):.2f}",
                flush=True,
            )
            torch.save(
                {"step": step + 1, "model": model.state_dict(), "optimizer": optimizer.state_dict(), "vocab": vocab, "cfg": model.cfg},
                ckpt_path,
            )

    fp = int.from_bytes(hashlib.sha256(f"{total_toks}:{len(vocab)}".encode()).digest()[:8], "little")
    pretrain_out = Path(args.out_pretrain_bin) if args.out_pretrain_bin else Path(args.out_bin)
    meta_pre = write_rlnm0001(pretrain_out, vocab, model.cfg, model.export_numpy_tensors(), corpus_fingerprint=fp)
    print(f"[export pretrain] RLNM0001 匯出完成: {meta_pre}", flush=True)

    if args.eval_nbest and (args.out_pretrain_bin or args.contrastive_steps <= 0):
        print("[eval pretrain] 評估預訓練模型於 dev N-Best 表現...", flush=True)
        evaluate_nbest_with_torch(model, vocab, Path(args.eval_nbest), device, amp_dtype if use_amp else None)

    if args.contrastive_steps > 0:
        if not lex_path or not lex_path.is_file():
            print("[錯誤] --contrastive-steps > 0 需要提供 --lexicon <lexicon_core.tsv>", file=sys.stderr)
            return 2
        run_contrastive_finetune(
            model, vocab, train_mm, train_bos, train_lens, lex_path, args, device, amp_dtype, use_amp, block_size
        )
        fp_c = int.from_bytes(hashlib.sha256(f"{total_toks}:{len(vocab)}:c{args.contrastive_steps}".encode()).digest()[:8], "little")
        meta_fin = write_rlnm0001(args.out_bin, vocab, model.cfg, model.export_numpy_tensors(), corpus_fingerprint=fp_c)
        print(f"[export contrast] RLNM0001 匯出完成: {meta_fin}", flush=True)
        if args.eval_nbest:
            print("[eval contrast] 評估對比微調模型於 dev N-Best 表現...", flush=True)
            evaluate_nbest_with_torch(model, vocab, Path(args.eval_nbest), device, amp_dtype if use_amp else None)
    return 0


def evaluate_nbest_with_torch(model, vocab: list[str], nbest_path: Path, device, amp_dtype) -> None:
    import torch
    import torch.nn.functional as F

    c2i = {c: i for i, c in enumerate(vocab)}
    max_len = int(model.cfg["max_seq_len"])
    records = []
    with open(nbest_path, "r", encoding="utf-8") as f:
        for line in f:
            s = line.strip()
            if s:
                records.append(json.loads(s))

    model.eval()
    all_texts: list[str] = []
    sent_slices: list[tuple[int, int]] = []
    for rec in records:
        start = len(all_texts)
        for c in rec["candidates"]:
            all_texts.append(c["text"])
        sent_slices.append((start, len(all_texts)))

    neural_scores = [0.0] * len(all_texts)
    batch_sz = 128
    with torch.no_grad():
        for b0 in range(0, len(all_texts), batch_sz):
            batch_txt = all_texts[b0 : b0 + batch_sz]
            seqs = [[BOS_ID] + [c2i.get(ch, UNK_ID) for ch in t[: max_len - 1]] for t in batch_txt]
            t_max = max(len(s) for s in seqs)
            inp = torch.full((len(seqs), t_max - 1), PAD_ID, dtype=torch.long, device=device)
            tgt = torch.full((len(seqs), t_max - 1), -100, dtype=torch.long, device=device)
            for bi, seq in enumerate(seqs):
                l_eff = len(seq) - 1
                inp[bi, :l_eff] = torch.tensor(seq[:-1], dtype=torch.long)
                tgt[bi, :l_eff] = torch.tensor(seq[1:], dtype=torch.long)
            with torch.amp.autocast(device_type=device.type, dtype=amp_dtype or torch.float32, enabled=(amp_dtype is not None)):
                logits = model(inp)
                log_probs = F.log_softmax(logits.float(), dim=-1)
                mask = tgt.ne(-100)
                safe_tgt = tgt.masked_fill(~mask, 0)
                tok_lp = log_probs.gather(-1, safe_tgt.unsqueeze(-1)).squeeze(-1)
                seq_lp = (tok_lp * mask.float()).sum(dim=-1)
            for bi, val in enumerate(seq_lp.tolist()):
                neural_scores[b0 + bi] = val

    # 掃描 lambda_neural
    best_lam = 0.0
    best_err = 10**9
    for lam_step in range(0, 25):
        lam = lam_step * 0.25
        tot_chars = 0
        tot_err = 0
        exact = 0
        base_err = 0
        base_exact = 0
        fixed = 0
        broken = 0
        for rec, (s0, s1) in zip(records, sent_slices):
            gold = rec["gold"]
            tot_chars += len(gold)
            top1_txt = rec["top1"]
            e_base = char_levenshtein(top1_txt, gold)
            base_err += e_base
            if e_base == 0:
                base_exact += 1

            best_txt = top1_txt
            best_sc = -1e18
            for idx, cand in enumerate(rec["candidates"]):
                sc = cand["base_score"] + lam * neural_scores[s0 + idx]
                if sc > best_sc:
                    best_sc = sc
                    best_txt = cand["text"]
            e_new = char_levenshtein(best_txt, gold)
            tot_err += e_new
            if e_new == 0:
                exact += 1
            if e_base > 0 and e_new == 0:
                fixed += 1
            elif e_base == 0 and e_new > 0:
                broken += 1
        cer = (tot_err / max(1, tot_chars)) * 100.0
        print(
            f"[eval lam={lam:4.2f}] CER={cer:.2f}% ({tot_err}/{tot_chars}) "
            f"exact={exact}/{len(records)} ({exact / len(records) * 100:.2f}%) "
            f"fixed={fixed} broken={broken}",
            flush=True,
        )
        if tot_err < best_err:
            best_err = tot_err
            best_lam = lam
    print(f"[eval summary] best_lambda={best_lam:.2f}, best_char_errors={best_err}", flush=True)


def evaluate_nbest_numpy(bin_path: Path, nbest_path: Path, max_sentences: int = 200) -> int:
    model = load_rlnm0001_numpy(bin_path)
    records = []
    with open(nbest_path, "r", encoding="utf-8") as f:
        for line in f:
            s = line.strip()
            if s:
                records.append(json.loads(s))
                if max_sentences > 0 and len(records) >= max_sentences:
                    break
    for lam in (0.0, 0.5, 1.0, 1.5, 2.0):
        tot_chars = 0
        tot_err = 0
        for rec in records:
            gold = rec["gold"]
            tot_chars += len(gold)
            best_txt = rec["top1"]
            best_sc = -1e18
            for cand in rec["candidates"]:
                lp = score_candidate_rlnm0001_numpy(model, cand["text"])
                sc = cand["base_score"] + lam * lp
                if sc > best_sc:
                    best_sc = sc
                    best_txt = cand["text"]
            tot_err += char_levenshtein(best_txt, gold)
        print(f"[numpy eval n={len(records)} lam={lam:.2f}] CER={tot_err / max(1, tot_chars) * 100:.2f}% ({tot_err}/{tot_chars})")
    return 0


def parse_args() -> argparse.Namespace:
    ap = argparse.ArgumentParser(description="如喜（RuhI）v1.0 輕量神經重排序訓練與 RLNM0001 量化匯出工具")
    ap.add_argument("--self-test-format", action="store_true", help="於本機無 PyTorch 執行 RLNM0001 二進位與數學不變量自檢")
    ap.add_argument("--prepare-corpus", action="store_true", help="串流下載並清洗 12 組商業安全台灣正體語料至 --corpus-txt")
    ap.add_argument("--train", action="store_true", help="於 GPU 環境訓練 RuhI-CharLM-v1 並匯出 RLNM0001 INT8 模型")
    ap.add_argument("--eval-nbest-numpy", help="以純 NumPy 載入 --out-bin 驗證 N-Best JSONL 離線重排")

    ap.add_argument("--corpus-txt", default="outputs/v10_clean_corpus.txt", help="純文字語料句段檔（每行一句）")
    ap.add_argument("--dev-hashes", help="開發／測試評測句子 SHA-256 雜湊名單 (.txt)")
    ap.add_argument("--lexicon", help="核心詞庫路徑 (供補充高頻單字至詞表)")
    ap.add_argument("--work-dir", default="outputs/v10_charlm_work", help="訓練暫存與斷點目錄")
    ap.add_argument("--checkpoint", help="斷點檔案路徑 (.pt)")
    ap.add_argument("--resume", action="store_true", help="自既有斷點繼續訓練")
    ap.add_argument("--out-pretrain-bin", default="", help="預訓練完成後先行匯出 RLNM0001 之選填路徑（供 A/B 對照）")
    ap.add_argument("--out-bin", default="outputs/rlnm_zh_tw.bin", help="輸出 RLNM0001 INT8 模型路徑")
    ap.add_argument("--eval-nbest", help="訓練後對指定 dev N-Best JSONL 執行重排校準")

    ap.add_argument("--target-chars", type=int, default=190_000_000)
    ap.add_argument("--fineweb-timeout-sec", type=int, default=900)
    ap.add_argument("--max-trad-zh-docs", type=int, default=500_000)

    ap.add_argument("--vocab-size", type=int, default=7200)
    ap.add_argument("--d-model", type=int, default=512)
    ap.add_argument("--n-layers", type=int, default=6)
    ap.add_argument("--n-heads", type=int, default=8)
    ap.add_argument("--d-ff", type=int, default=1368)
    ap.add_argument("--max-seq-len", type=int, default=256)

    ap.add_argument("--batch-size", type=int, default=64)
    ap.add_argument("--grad-accum", type=int, default=2)
    ap.add_argument("--max-steps", type=int, default=12000)
    ap.add_argument("--warmup-steps", type=int, default=500)
    ap.add_argument("--lr", type=float, default=4e-4)
    ap.add_argument("--weight-decay", type=float, default=0.1)
    ap.add_argument("--cold-start-ratio", type=float, default=0.40, help="冷啟動單句／雙句前綴對齊訓練批次比例")
    ap.add_argument("--contrastive-steps", type=int, default=0, help="預訓練後於訓練集執行同音對比排序微調步數（0=停用）")
    ap.add_argument("--contrastive-lr", type=float, default=4e-5)
    ap.add_argument("--contrastive-margin", type=float, default=4.0)
    ap.add_argument("--contrastive-alpha", type=float, default=0.50)
    ap.add_argument("--bf16", action="store_true", help="於支援 Ampere+ 的 GPU 強制啟用 BF16 混合精度")
    ap.add_argument("--log-every", type=int, default=100)
    ap.add_argument("--eval-every", type=int, default=1000)
    ap.add_argument("--seed", type=int, default=20261124)
    return ap.parse_args()


def main() -> int:
    args = parse_args()
    if args.self_test_format:
        return run_self_test_format()
    if args.prepare_corpus:
        if not args.dev_hashes:
            print("[錯誤] --prepare-corpus 必須指定 --dev-hashes", file=sys.stderr)
            return 2
        res = prepare_clean_corpus_txt(
            Path(args.corpus_txt),
            Path(args.dev_hashes),
            target_chars=args.target_chars,
            fineweb_timeout_sec=args.fineweb_timeout_sec,
            max_trad_zh_docs=args.max_trad_zh_docs,
        )
        print(json.dumps({k: v for k, v in res.items() if k != "sources"}, ensure_ascii=False, indent=2))
        return 0
    if args.train:
        return run_torch_training(args)
    if args.eval_nbest_numpy:
        return evaluate_nbest_numpy(Path(args.out_bin), Path(args.eval_nbest_numpy))
    print("請指定執行模式（--self-test-format、--prepare-corpus、--train 或 --eval-nbest-numpy）；使用 --help 檢視說明。")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
