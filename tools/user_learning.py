#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
如喜沙盒輸入座 (RuhI Sandbox Input Seat)
user_learning.py — Gboard 級使用者擊鍵動態頻率學習引擎
=============================================================================
功能：
1. 記錄使用者選字、選詞之歷史次數 (Hit Count) 與最後使用時間 (Last Used)
2. 越常被選取的同音字、同音詞，動態調高排序權重，自動排在候選列最前面
3. 持久化存儲於 user_learning.json，輕量高效
"""

import json
import os
import time
from typing import Dict, List, Optional, Tuple


class DynamicLearningStore:
    """
    動態詞頻與選字學習管理器
    """

    def __init__(self, storage_path: Optional[str] = None):
        if storage_path is None:
            base_dir = os.path.dirname(os.path.abspath(__file__))
            storage_path = os.path.join(base_dir, "user_learning.json")
        self.storage_path = os.path.abspath(storage_path)

        # 結構：key = f"{bpmf}:{word}" -> {"hits": int, "last_used": float}
        self.history: Dict[str, dict] = {}
        self.load()

    def _make_key(self, bpmf: str, word: str) -> str:
        norm_bpm = " ".join(bpmf.replace("-", " ").split()).strip()
        return f"{norm_bpm}:{word.strip()}"

    def load(self):
        """載入學習歷史"""
        self.history.clear()
        if not os.path.exists(self.storage_path):
            return

        try:
            with open(self.storage_path, "r", encoding="utf-8") as f:
                data = json.load(f)
                if isinstance(data, dict):
                    self.history = data
        except Exception as e:
            print(f"[user_learning] 載入學習記錄失敗: {e}")

    def save(self):
        """保存學習歷史"""
        try:
            os.makedirs(os.path.dirname(self.storage_path), exist_ok=True)
            with open(self.storage_path, "w", encoding="utf-8") as f:
                json.dump(self.history, f, ensure_ascii=False, indent=2)
        except Exception as e:
            print(f"[user_learning] 儲存學習記錄失敗: {e}")

    def record_selection(self, bpmf: str, word: str):
        """
        記錄使用者選取了某個同音字/同音詞
        """
        word = word.strip()
        if not word:
            return

        key = self._make_key(bpmf, word)
        now = time.time()

        if key not in self.history:
            self.history[key] = {"hits": 1, "last_used": now}
        else:
            self.history[key]["hits"] = self.history[key].get("hits", 0) + 1
            self.history[key]["last_used"] = now

        # 即時持久化
        self.save()

    def get_hits(self, bpmf: str, word: str) -> int:
        """取得該注音下該字詞的選取次數"""
        key = self._make_key(bpmf, word)
        entry = self.history.get(key)
        if entry:
            return entry.get("hits", 0)
        return 0

    def sort_candidates(self, bpmf: str, candidates: List[str], custom_words_set: Optional[set] = None) -> List[str]:
        """
        依動態學習次數 > 自建詞 > 原始順序 進行加權重排
        """
        if not candidates:
            return []

        custom_set = custom_words_set or set()

        def rank_score(item: str) -> Tuple[int, int, float]:
            hits = self.get_hits(bpmf, item)
            is_custom = 1 if item in custom_set else 0
            key = self._make_key(bpmf, item)
            last_used = self.history.get(key, {}).get("last_used", 0.0)
            return (hits, is_custom, last_used)

        # 穩定排序：高分在前
        return sorted(candidates, key=rank_score, reverse=True)
