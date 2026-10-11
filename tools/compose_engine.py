#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
如喜沙盒輸入座 (RuhI Sandbox Input Seat)
compose_engine.py — 歐文 Compose Key 引擎
=============================================================================
支援使用者自訂的雙鍵組合輸入歐文字母（如 ' + a → á）。
同時提供字母變體清單（如手機輸入法般的長按彈出）。
"""

import os
from typing import Dict, List, Optional, Tuple


# 字母變體對照表 (Character variant lookup table)
CHAR_VARIANTS: Dict[str, List[str]] = {
    "a": ["a", "á", "à", "â", "ã", "ä", "å", "ā", "ă", "ą", "æ"],
    "A": ["A", "Á", "À", "Â", "Ã", "Ä", "Å", "Ā", "Ă", "Ą", "Æ"],
    "c": ["c", "ç", "ć", "č"],
    "C": ["C", "Ç", "Ć", "Č"],
    "e": ["e", "é", "è", "ê", "ë", "ē", "ĕ", "ę"],
    "E": ["E", "É", "È", "Ê", "Ë", "Ē", "Ĕ", "Ę"],
    "i": ["i", "í", "ì", "î", "ï", "ī", "ĭ"],
    "I": ["I", "Í", "Ì", "Î", "Ï", "Ī", "Ĭ"],
    "n": ["n", "ñ", "ń", "ň"],
    "N": ["N", "Ñ", "Ń", "Ň"],
    "o": ["o", "ó", "ò", "ô", "õ", "ö", "ō", "ő", "ø"],
    "O": ["O", "Ó", "Ò", "Ô", "Õ", "Ö", "Ō", "Ő", "Ø"],
    "s": ["s", "š", "ś", "ß"],
    "S": ["S", "Š", "Ś"],
    "u": ["u", "ú", "ù", "û", "ü", "ū", "ŭ", "ű"],
    "U": ["U", "Ú", "Ù", "Û", "Ü", "Ū", "Ŭ", "Ű"],
    "y": ["y", "ý", "ÿ"],
    "Y": ["Y", "Ý", "Ÿ"],
    "z": ["z", "ž", "ź", "ż"],
    "Z": ["Z", "Ž", "Ź", "Ż"],
}


class ComposeEngine:
    """
    歐文 Compose Key 引擎 (European Compose Key Engine)
    
    用法：
        engine = ComposeEngine()
        engine.load_table("compose_tables/default.tsv")
        
        # 雙鍵組合
        result = engine.try_compose("'", "a")  # → "á"
        
        # 字母變體清單
        variants = engine.get_variants("a")  # → ["a", "á", "à", ...]
    """

    def __init__(self):
        # 組合對照表：(trigger1, trigger2) → output
        self._table: Dict[Tuple[str, str], str] = {}
        # 所有 trigger1 字元集合（用於快速判斷是否為 compose 開頭）
        self._triggers: set = set()
        # 目前暫存的第一個 trigger（等待第二個字元）
        self._pending_trigger: Optional[str] = None

    def load_table(self, path: str) -> int:
        """
        載入 compose 對照表 TSV (Load compose mapping table)
        
        格式: trigger1\ttrigger2\toutput\tdescription
        # 開頭為註解
        
        Returns:
            載入筆數
        """
        self._table.clear()
        self._triggers.clear()
        count = 0

        if not os.path.exists(path):
            print(f"[compose_engine] 對照表不存在: {path}")
            return 0

        try:
            with open(path, "r", encoding="utf-8") as f:
                for line in f:
                    line = line.strip()
                    if not line or line.startswith("#"):
                        continue
                    parts = line.split("\t")
                    if len(parts) >= 3:
                        t1, t2, output = parts[0], parts[1], parts[2]
                        self._table[(t1, t2)] = output
                        self._triggers.add(t1)
                        count += 1
        except Exception as e:
            print(f"[compose_engine] 載入失敗: {e}")

        return count

    def is_compose_trigger(self, ch: str) -> bool:
        """判斷字元是否為 compose 序列的起始鍵 (Check if char is a compose trigger)"""
        return ch in self._triggers

    def has_pending(self) -> bool:
        """是否有暫存的第一個 trigger (Check if there's a pending trigger)"""
        return self._pending_trigger is not None

    def feed(self, ch: str) -> Tuple[Optional[str], bool]:
        """
        餵入一個字元至 compose 引擎 (Feed a character to compose engine)
        
        Returns:
            (result, consumed)
            - result: 組合結果字元，或 None
            - consumed: 是否消耗了此字元
            
        流程：
            1. 無暫存：如果 ch 是 trigger → 暫存，回傳 (None, True)
            2. 有暫存：嘗試 (pending, ch) 組合
               - 成功 → 回傳 (output, True)
               - 失敗 → 回傳 (None, False)，同時清空暫存
        """
        if self._pending_trigger is not None:
            # 嘗試組合 (Try composing)
            key = (self._pending_trigger, ch)
            result = self._table.get(key)
            self._pending_trigger = None

            if result:
                return (result, True)
            else:
                # 組合失敗，回傳 None 表示需要把暫存的 trigger 也送出
                return (None, False)
        else:
            # 檢查是否為 compose trigger (Check if compose trigger)
            if self.is_compose_trigger(ch):
                self._pending_trigger = ch
                return (None, True)
            else:
                return (None, False)

    def cancel(self) -> Optional[str]:
        """
        取消目前的 compose 暫存 (Cancel pending compose)
        
        Returns:
            被取消的 trigger 字元（需要補送給引擎），或 None
        """
        t = self._pending_trigger
        self._pending_trigger = None
        return t

    def try_compose(self, t1: str, t2: str) -> Optional[str]:
        """直接嘗試組合兩個字元 (Try composing two characters directly)"""
        return self._table.get((t1, t2))

    def get_variants(self, ch: str) -> List[str]:
        """取得字元的所有變體（如手機輸入法長按）(Get all variants of a character)"""
        return CHAR_VARIANTS.get(ch, [ch])

    def get_all_compose_results_for(self, ch: str) -> List[Tuple[str, str]]:
        """
        取得某字元作為 trigger2 的所有可能組合結果
        (Get all compose results where ch is the second trigger)
        
        Returns:
            [(trigger1 + description, output), ...]
        """
        results = []
        for (t1, t2), output in self._table.items():
            if t2 == ch:
                results.append((f"{t1}+{t2}", output))
        return results


# === 測試 (Test) ===
if __name__ == "__main__":
    import sys
    sys.stdout.reconfigure(encoding="utf-8")

    engine = ComposeEngine()
    table_path = os.path.join(os.path.dirname(os.path.abspath(__file__)), "compose_tables", "default.tsv")
    count = engine.load_table(table_path)
    print(f"載入 {count} 筆 compose 映射")

    # 測試直接組合
    tests = [("'", "a"), ("'", "e"), ("~", "n"), ("_", "a"), ("^", "o")]
    for t1, t2 in tests:
        result = engine.try_compose(t1, t2)
        print(f"  {t1} + {t2} → {result}")

    # 測試 feed 模式
    print("\nFeed 模式測試：")
    engine2 = ComposeEngine()
    engine2.load_table(table_path)
    for ch in ["'", "u"]:
        result, consumed = engine2.feed(ch)
        print(f"  feed('{ch}') → result={result}, consumed={consumed}")

    # 測試變體
    print("\n字母變體：")
    for ch in ["a", "n", "u"]:
        variants = engine.get_variants(ch)
        print(f"  {ch} → {variants}")
