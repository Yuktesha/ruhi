#!/usr/bin/env python3
# -*- coding: utf-8 -*-

import sys
from pathlib import Path
_ROOT = Path(__file__).resolve().parent.parent
_TOOLS = _ROOT / "tools"
for p in [str(_ROOT), str(_TOOLS)]:
    if p not in sys.path:
        sys.path.insert(0, p)
"""
ruhi_sandbox/test_phase_8.py
驗證 Phase 8: 存現句與動形重疊生動構式 (Existential & Reduplication Topology)
依據陸儉明《現代漢語語法研究教程》與劉月華《實用現代漢語語法》：
1. 空間存現句（處所詞 + 存現動詞 + 著/了 + 存現事物）
   - 桌上放著一本書、門口站著一位客人、牆上掛著一幅畫、空中飄著朵朵白雲、黑板上寫著幾行字、桌上擺滿了水果、院子裡種著許多花
2. 動詞生動重疊（嘗試態/緩和語氣: AA, A一A, ABAB）
   - 試試看、看一看、想了一想、研究研究、商量商量、考慮考慮、放鬆放鬆
3. 形容詞生動重疊（加深程度/描摹狀態: AABB, ABB）
   - 漂漂亮亮、乾乾淨淨、高高興興、平平安安、明明白白、綠油油、紅通通、沉甸甸、慢吞吞
"""
import os
import sys
import ctypes
import json
from pathlib import Path

sys.stdout.reconfigure(encoding='utf-8')

from test_phase_1 import get_hsu_keys_for_syllables

PHASE_8_CASES = [
    # ── 空間存現句（Existential Sentences） ──
    ("桌上放著一本書", "ㄓㄨㄛ ㄕㄤˋ ㄈㄤˋ ˙ㄓㄜ ㄧ ㄅㄣˇ ㄕㄨ"),
    ("門口站著一位客人", "ㄇㄣˊ ㄎㄡˇ ㄓㄢˋ ˙ㄓㄜ ㄧ ㄨㄟˋ ㄎㄜˋ ㄖㄣˊ"),
    ("牆上掛著一幅畫", "ㄑㄧㄤˊ ㄕㄤˋ ㄍㄨㄚˋ ˙ㄓㄜ ㄧ ㄈㄨˊ ㄏㄨㄚˋ"),
    ("空中飄著朵朵白雲", "ㄎㄨㄥ ㄓㄨㄥ ㄆㄧㄠ ˙ㄓㄜ ㄉㄨㄛˇ ㄉㄨㄛˇ ㄅㄞˊ ㄩㄣˊ"),
    ("黑板上寫著幾行字", "ㄏㄟ ㄅㄢˇ ㄕㄤˋ ㄒㄧㄝˇ ˙ㄓㄜ ㄐㄧˇ ㄏㄤˊ ㄗˋ"),
    ("桌上擺滿了水果", "ㄓㄨㄛ ㄕㄤˋ ㄅㄞˇ ㄇㄢˇ ˙ㄌㄜ ㄕㄨㄟˇ ㄍㄨㄛˇ"),
    ("院子裡種著許多花", "ㄩㄢˋ ˙ㄗ ㄌㄧˇ ㄓㄨㄥˋ ˙ㄓㄜ ㄒㄩˇ ㄉㄨㄛ ㄏㄨㄚ"),

    # ── 動詞生動重疊（Verb Reduplication） ──
    ("試試看", "ㄕˋ ㄕˋ ㄎㄢˋ"),
    ("看一看", "ㄎㄢˋ ㄧ ㄎㄢˋ"),
    ("想了一想", "ㄒㄧㄤˇ ˙ㄌㄜ ㄧ ㄒㄧㄤˇ"),
    ("研究研究", "ㄧㄢˊ ㄐㄧㄡˋ ㄧㄢˊ ㄐㄧㄡˋ"),
    ("商量商量", "ㄕㄤ ˙ㄌㄧㄤ ㄕㄤ ˙ㄌㄧㄤ"),
    ("考慮考慮", "ㄎㄠˇ ㄌㄩˋ ㄎㄠˇ ㄌㄩˋ"),
    ("放鬆放鬆", "ㄈㄤˋ ㄙㄨㄥ ㄈㄤˋ ㄙㄨㄥ"),

    # ── 形容詞生動重疊（Adjective Reduplication: AABB & ABB） ──
    ("漂漂亮亮", "ㄆㄧㄠˋ ㄆㄧㄠˋ ㄌㄧㄤˋ ㄌㄧㄤˋ"),
    ("乾乾淨淨", "ㄍㄢ ㄍㄢ ㄐㄧㄥˋ ㄐㄧㄥˋ"),
    ("高高興興", "ㄍㄠ ㄍㄠ ㄒㄧㄥˋ ㄒㄧㄥˋ"),
    ("平平安安", "ㄆㄧㄥˊ ㄆㄧㄥˊ ㄢ ㄢ"),
    ("明明白白", "ㄇㄧㄥˊ ㄇㄧㄥˊ ㄅㄞˊ ㄅㄞˊ"),
    ("綠油油", "ㄌㄩˋ ㄧㄡˊ ㄧㄡˊ"),
    ("紅通通", "ㄏㄨㄥˊ ㄊㄨㄥ ㄊㄨㄥ"),
    ("沉甸甸", "ㄔㄣˊ ㄉㄧㄢˋ ㄉㄧㄢˋ"),
    ("慢吞吞", "ㄇㄢˋ ㄊㄨㄣ ㄊㄨㄣ"),
]

def run_tests():
    dll_path = os.path.abspath("C:/ATGprjs/ruhi/ruhi-core/target/release/ruhi.dll")
    lib = ctypes.CDLL(dll_path)
    lib.ruhi_engine_create.restype = ctypes.c_void_p
    lib.ruhi_engine_load_tsv.argtypes = [ctypes.c_void_p, ctypes.c_char_p]
    lib.ruhi_engine_load_transitions_tsv.argtypes = [ctypes.c_void_p, ctypes.c_char_p]
    lib.ruhi_engine_load_transitions_tsv.restype = ctypes.c_int
    lib.ruhi_engine_process_key.argtypes = [ctypes.c_void_p, ctypes.c_char]
    lib.ruhi_engine_process_key.restype = ctypes.c_char_p
    lib.ruhi_engine_reset.argtypes = [ctypes.c_void_p]

    engine = lib.ruhi_engine_create()
    tsv_path = "C:/ATGprjs/ruhi/ruhi_lexicon/data/lexicon_moe.tsv".encode("utf-8")
    lib.ruhi_engine_load_tsv(engine, tsv_path)

    dom_path = "C:/ATGprjs/ruhi/ruhi_lexicon/data/lexicon_domain.tsv".encode("utf-8")
    if os.path.exists("C:/ATGprjs/ruhi/ruhi_lexicon/data/lexicon_domain.tsv"):
        lib.ruhi_engine_load_tsv(engine, dom_path)

    trans_path = "C:/ATGprjs/ruhi/ruhi_lexicon/data/transitions_user.tsv".encode("utf-8")
    if os.path.exists("C:/ATGprjs/ruhi/ruhi_lexicon/data/transitions_user.tsv"):
        lib.ruhi_engine_load_transitions_tsv(engine, trans_path)

    print("=" * 70)
    print("🏡 Phase 8 (存現句與動形重疊生動構式 - 陸儉明理論) 驗證開始")
    print("=" * 70)

    passed = 0
    failed = 0
    for expected, bpm_str in PHASE_8_CASES:
        clean_exp = expected.split("(")[0]
        syls = bpm_str.split()
        keys = get_hsu_keys_for_syllables(syls)

        lib.ruhi_engine_reset(engine)
        output = ""
        for k in keys:
            ptr = lib.ruhi_engine_process_key(engine, k.encode('ascii'))
            if ptr:
                res = json.loads(ctypes.cast(ptr, ctypes.c_char_p).value.decode('utf-8'))
                if res.get("action", "").startswith("commit"):
                    output += res.get("committed_chunk", "")
                elif res.get("composed"):
                    output = res.get("composed")

        is_match = (clean_exp == output)
        if is_match:
            passed += 1
            print(f"✅ PASS: [{clean_exp}]")
        else:
            failed += 1
            print(f"❌ FAIL: 目標 [{clean_exp}] != 輸出 [{output}]")

    print("=" * 70)
    pct = (passed / len(PHASE_8_CASES)) * 100.0
    print(f"總結: {passed} / {len(PHASE_8_CASES)} 通過 (通過率: {pct:.1f}%)")
    if failed == 0:
        print("🎉 Phase 8 完美全數通關！")
        return 0
    else:
        return 1

if __name__ == "__main__":
    sys.exit(run_tests())
