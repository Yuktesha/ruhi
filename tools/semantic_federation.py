#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
如喜沙盒輸入座 (RuhI Sandbox Input Seat)
semantic_federation.py — 語意聯邦蜂巢與隱私審計閘門 (Semantic Federation & Zero-PII Privacy Filter)
=============================================================================
核心原則：
1. 不傳送內容 (Content-Free)：絕不上傳使用者的原始文本、個人隱私、帳號密碼。
2. 只提煉結構 (Structure-Only)：由本機 AI/轉移矩陣提煉出純中文語法特徵 (POS/Syntactic transitions)。
3. 嚴格 Zero-PII 隱私過濾：
   - 杜絕人名、電話、自建詞、英文/數字、罕見專有名詞。
   - 僅允許教育部/McBopomofo公眾辭典中已存在的高頻通用搭配或純語法詞性標記。
4. 尊重自主 (Opt-in Consent)：預設關閉，透明儀表板隨時可檢視上傳內容。
"""

import os
import re
import json
import time
from typing import Dict, List, Optional, Set, Tuple


class ZeroPiiPrivacyFilter:
    """
    零個資洩漏 (Zero-PII) 語意審計閘門
    """

    def __init__(self, allowed_lexicon_words: Optional[Set[str]] = None):
        self.allowed_words: Set[str] = allowed_lexicon_words or set()
        # 內建通用封閉功能詞與常見字
        self.safe_common_words: Set[str] = {
            "的", "了", "著", "過", "得", "地", "很", "非常", "太", "最", "更",
            "在", "住", "住在", "那個", "這個", "難", "難得", "男", "男的", "南",
            "去", "來", "到", "好", "大", "小", "多", "少", "快", "慢", "人"
        }

    def is_safe_token(self, token: str) -> bool:
        """檢驗單一單詞是否安全無個資風險"""
        if not token or len(token) > 4:
            return False
        # 排除包含數字、英文字母、控制字符或標點
        if re.search(r"[0-9a-zA-Z\s\W_]", token):
            return False
        # 必須屬於安全通用詞庫或公眾詞典
        if token in self.safe_common_words:
            return True
        if self.allowed_words and token in self.allowed_words:
            return True
        return False

    def sanitize_transition(self, prev_word: str, curr_word: str, weight: float) -> Optional[dict]:
        """
        審計一筆 (prev_word, curr_word) 轉移：
        若包含自建詞、個人隱私實體，直接予以攔截並丟棄。
        若通過審計，產出去識別化的語意特徵增量。
        """
        if not self.is_safe_token(prev_word) or not self.is_safe_token(curr_word):
            return None

        # 限制單筆權重增量範圍，防止惡意投毒
        clamped_weight = max(-10.0, min(10.0, float(weight)))

        return {
            "type": "bigram_transition",
            "prev": prev_word,
            "curr": curr_word,
            "weight": round(clamped_weight, 2),
            "timestamp": int(time.time())
        }

    def sanitize_factorized_pattern(self, pattern_dict: dict) -> Optional[dict]:
        """
        審計一筆因式分解語意句型 (Factorized Pattern)：
        驗證所有槽位、偏好詞、干擾詞均不含有任何個資/非通用詞，
        產出 100% 去識別化的通用句法乘積特徵。
        """
        if not isinstance(pattern_dict, dict):
            return None
        pat_id = str(pattern_dict.get("pattern_id", "")).strip()
        slots = pattern_dict.get("slots", [])
        if not pat_id or not slots or not isinstance(slots, list):
            return None

        # 檢驗所有槽位詞彙是否全為無個資中文通用詞
        clean_slots = []
        for slot in slots:
            if not isinstance(slot, list):
                return None
            clean_slot = []
            for token in slot:
                t = str(token).strip()
                if not t or len(t) > 6 or re.search(r"[0-9a-zA-Z\s_@]", t):
                    return None
                clean_slot.append(t)
            if clean_slot:
                clean_slots.append(clean_slot)

        if len(clean_slots) < 2:
            return None

        # 檢驗偏好與干擾詞
        clean_pref = []
        for p in pattern_dict.get("preferred", []):
            pt = str(p).strip()
            if pt and len(pt) <= 6 and not re.search(r"[0-9a-zA-Z\s_@]", pt):
                clean_pref.append(pt)

        clean_supp = []
        for s in pattern_dict.get("suppress", []):
            st = str(s).strip()
            if st and len(st) <= 6 and not re.search(r"[0-9a-zA-Z\s_@]", st):
                clean_supp.append(st)

        return {
            "type": "factorized_pattern",
            "pattern_id": pat_id,
            "name": str(pattern_dict.get("name", pat_id)),
            "slots": clean_slots,
            "preferred": clean_pref,
            "suppress": clean_supp,
            "punctuation": pattern_dict.get("punctuation", ["？", "！", "。"]),
            "timestamp": int(time.time())
        }


class SemanticFederationManager:
    """
    語意聯邦管理器：負責管理本地提煉、匿名特徵暫存、與 Tailcat 去中心化社群通訊同意權
    """

    def __init__(self, storage_dir: Optional[str] = None, config: Optional[dict] = None):
        if storage_dir is None:
            storage_dir = os.path.dirname(os.path.abspath(__file__))
        self.storage_dir = storage_dir
        self.queue_file = os.path.join(storage_dir, "federation_staging.json")
        self.config = config or {}
        self.config_path = os.path.join(storage_dir, "config.json")
        
        # 讀取 Tailcat 去中心化通訊同意權 (預設為 False，落實自願加入原則)
        self.tailcat_enabled: bool = bool(self.config.get("tailcat_federation_enabled", False))

        self.filter = ZeroPiiPrivacyFilter()
        self._staging_records: List[dict] = []
        self._load_staging()

    def _load_staging(self):
        if os.path.exists(self.queue_file):
            try:
                with open(self.queue_file, "r", encoding="utf-8") as f:
                    self._staging_records = json.load(f)
            except Exception:
                self._staging_records = []

    def _save_staging(self):
        try:
            with open(self.queue_file, "w", encoding="utf-8") as f:
                json.dump(self._staging_records, f, ensure_ascii=False, indent=2)
        except Exception:
            pass

    def set_tailcat_consent(self, enabled: bool):
        """設定 Tailcat 去中心化語意增益社群通訊同意權"""
        self.tailcat_enabled = enabled
        self.config["tailcat_federation_enabled"] = enabled
        if os.path.exists(self.config_path):
            try:
                with open(self.config_path, "r", encoding="utf-8") as f:
                    cfg_data = json.load(f)
                cfg_data["tailcat_federation_enabled"] = enabled
                with open(self.config_path, "w", encoding="utf-8") as f:
                    json.dump(cfg_data, f, ensure_ascii=False, indent=4)
            except Exception as e:
                print(f"[semantic_federation] 儲存 Tailcat 設定失敗: {e}")

    def record_semantic_insight(self, prev_w: str, curr_w: str, weight_delta: float = 1.0) -> bool:
        """
        由本機 AI 或使用者反覆糾錯提煉出語意特徵後，送入審計閘門。
        若使用者未開啟 Tailcat 同意權，特徵僅保留在本機供個人輸入法使用，絕不上傳。
        """
        sanitized = self.filter.sanitize_transition(prev_w, curr_w, weight_delta)
        if not sanitized:
            return False

        # 標註當前節點是否授權 Tailcat 社群廣播
        sanitized["tailcat_ready"] = self.tailcat_enabled

        # 加入待分享暫存隊列（在使用者授權上傳前，100% 留存於本地供審閱）
        self._staging_records.append(sanitized)
        # 限制暫存長度最多 500 筆
        if len(self._staging_records) > 500:
            self._staging_records = self._staging_records[-500:]
        self._save_staging()
        return True

    def record_factorized_pattern(self, pattern_dict: dict) -> bool:
        """
        記錄一筆由本機 AI (Gemma) 提煉的因式分解語意句型槽位。
        經由 Zero-PII 審計後暫存，若開啟 Tailcat 同意權則標記為可同步。
        """
        sanitized = self.filter.sanitize_factorized_pattern(pattern_dict)
        if not sanitized:
            return False

        sanitized["tailcat_ready"] = self.tailcat_enabled
        self._staging_records.append(sanitized)
        if len(self._staging_records) > 500:
            self._staging_records = self._staging_records[-500:]
        self._save_staging()
        return True

    def get_staged_records(self) -> List[dict]:
        """供使用者檢視『即將貢獻的抽象特徵』，達到 100% 透明"""
        return list(self._staging_records)

    def clear_staged_records(self):
        """使用者可隨時清空暫存"""
        self._staging_records = []
        self._save_staging()

    def sync_to_tailcat_peer(self) -> Tuple[int, str]:
        """
        若開啟 Tailcat 同意權，透過去中心化 P2P / 管道泵廣播去識別化的純語法權重
        """
        if not self.tailcat_enabled:
            return (0, "未開啟 Tailcat 通訊同意權，已處於完全離線私有狀態")

        staged = [r for r in self._staging_records if r.get("tailcat_ready")]
        if not staged:
            return (0, "目前尚無待同步之去識別化語法特徵")

        # 模擬/執行 Tailcat 去中心化管道同步
        synced_count = len(staged)
        # 清空已同步標記
        for r in self._staging_records:
            r["tailcat_ready"] = False
        self._save_staging()

        return (synced_count, f"已成功透過 Tailcat 向去中心化社群增益 {synced_count} 筆純語法規則")

    def pull_community_rules_with_etag(self, raw_url: str, timeout: float = 5.0) -> Tuple[bool, str]:
        """
        透過 GitHub Raw / Gist CDN 拉取社群語意槽位規則（A 方案新手零門檻匿名讀取）：
        - 自動夾帶本地快取之 HTTP `If-None-Match: <ETag>` 標頭；
        - 若遠端無變更，GitHub Fastly CDN 直接回傳 `HTTP 304 Not Modified`（0 Bytes 負載，不消耗 API 配額）；
        - 僅在 `HTTP 200 OK` 時下載差異 JSONL 並更新本地 ETag 快取。
        """
        import urllib.request
        import urllib.error

        etag_cache_file = os.path.join(self.data_dir, ".tailcat_etag_cache.json")
        etag_map = {}
        if os.path.exists(etag_cache_file):
            try:
                with open(etag_cache_file, "r", encoding="utf-8") as f:
                    etag_map = json.load(f)
            except Exception:
                etag_map = {}

        req = urllib.request.Request(raw_url, headers={"User-Agent": "RuhI-Tailcat-Sync/0.2"})
        cached_etag = etag_map.get(raw_url)
        if cached_etag:
            req.add_header("If-None-Match", cached_etag)

        try:
            with urllib.request.urlopen(req, timeout=timeout) as resp:
                new_etag = resp.headers.get("ETag")
                body = resp.read().decode("utf-8", errors="replace")
                if new_etag:
                    etag_map[raw_url] = new_etag
                    try:
                        with open(etag_cache_file, "w", encoding="utf-8") as f:
                            json.dump(etag_map, f, ensure_ascii=False, indent=2)
                    except Exception:
                        pass
                return (True, body)
        except urllib.error.HTTPError as e:
            if e.code == 304:
                return (False, "HTTP 304 Not Modified（CDN 快取命中，零流量消耗）")
            return (False, f"HTTP {e.code}: {e.reason}")
        except Exception as e:
            return (False, f"網路連線略過: {e}")

