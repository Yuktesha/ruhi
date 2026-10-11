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
ruhi_sandbox/test_phase_7.py
驗證 Phase 7: 兼語式與連動結構引擎 (Pivotal & Serial Verb Constructions)
依據趙元任《中國話的文法》(A Grammar of Spoken Chinese) 與呂叔湘《現代漢語八百詞》：
1. 兼語式（Pivotal Construction: 致使/命使/邀請動詞 + 兼語 + 後續動詞）
   - 叫我一聲、叫他快來、請他幫忙、請大家坐下、讓大家放心、讓他進來、使人感動、令人佩服、派人前往、催他快走、陪他聊天
2. 連動結構（Serial Verb Construction: 目的/方式/工具/伴隨連續動詞短語）
   - 去圖書館借書、來台北找朋友、上街買菜、坐高鐵回台北、拿筆寫字、開車去上班、用筷子吃飯、閉著眼睛聽音樂、笑著對我說、站著等公車
"""
import os
import sys
import ctypes
import json
from pathlib import Path

sys.stdout.reconfigure(encoding='utf-8')

from test_phase_1 import get_hsu_keys_for_syllables

PHASE_7_CASES = [
    # ── 兼語式（Pivotal Constructions） ──
    ("叫我一聲", "ㄐㄧㄠˋ ㄨㄛˇ ㄧ ㄕㄥ"),
    ("叫他快來", "ㄐㄧㄠˋ ㄊㄚ ㄎㄨㄞˋ ㄌㄞˊ"),
    ("請他幫忙", "ㄑㄧㄥˇ ㄊㄚ ㄅㄤ ㄇㄤˊ"),
    ("請大家坐下", "ㄑㄧㄥˇ ㄉㄚˋ ㄐㄧㄚ ㄗㄨㄛˋ ㄒㄧㄚˋ"),
    ("讓大家放心", "ㄖㄤˋ ㄉㄚˋ ㄐㄧㄚ ㄈㄤˋ ㄒㄧㄣ"),
    ("讓他進來", "ㄖㄤˋ ㄊㄚ ㄐㄧㄣˋ ㄌㄞˊ"),
    ("使人感動", "ㄕˇ ㄖㄣˊ ㄍㄢˇ ㄉㄨㄥˋ"),
    ("令人佩服", "ㄌㄧㄥˋ ㄖㄣˊ ㄆㄟˋ ㄈㄨˊ"),
    ("派人前往", "ㄆㄞˋ ㄖㄣˊ ㄑㄧㄢˊ ㄨㄤˇ"),
    ("催他快走", "ㄘㄨㄟ ㄊㄚ ㄎㄨㄞˋ ㄗㄡˇ"),
    ("陪他聊天", "ㄆㄟˊ ㄊㄚ ㄌㄧㄠˊ ㄊㄧㄢ"),

    # ── 連動結構（Serial Verb Constructions） ──
    ("去圖書館借書", "ㄑㄩˋ ㄊㄨˊ ㄕㄨ ㄍㄨㄢˇ ㄐㄧㄝˋ ㄕㄨ"),
    ("來台北找朋友", "ㄌㄞˊ ㄊㄞˊ ㄅㄟˇ ㄓㄠˇ ㄆㄥˊ ㄧㄡˇ"),
    ("上街買菜", "ㄕㄤˋ ㄐㄧㄝ ㄇㄞˇ ㄘㄞˋ"),
    ("坐高鐵回台北", "ㄗㄨㄛˋ ㄍㄠ ㄊㄧㄝˇ ㄏㄨㄟˊ ㄊㄞˊ ㄅㄟˇ"),
    ("拿筆寫字", "ㄋㄚˊ ㄅㄧˇ ㄒㄧㄝˇ ㄗˋ"),
    ("開車去上班", "ㄎㄞ ㄔㄜ ㄑㄩˋ ㄕㄤˋ ㄅㄢ"),
    ("用筷子吃飯", "ㄩㄥˋ ㄎㄨㄞˋ ˙ㄗ ㄔ ㄈㄢˋ"),
    ("閉著眼睛聽音樂", "ㄅㄧˋ ˙ㄓㄜ ㄧㄢˇ ㄐㄧㄥ ㄊㄧㄥ ㄧㄣ ㄩㄝˋ"),
    ("笑著對我說", "ㄒㄧㄠˋ ˙ㄓㄜ ㄉㄨㄟˋ ㄨㄛˇ ㄕㄨㄛ"),
    ("站著等公車", "ㄓㄢˋ ˙ㄓㄜ ㄉㄥˇ ㄍㄨㄥ ㄔㄜ"),
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
    print("🎭 Phase 7 (兼語式與連動結構引擎 - 趙元任《中國話的文法》) 驗證開始")
    print("=" * 70)

    passed = 0
    failed = 0
    for expected, bpm_str in PHASE_7_CASES:
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
    pct = (passed / len(PHASE_7_CASES)) * 100.0
    print(f"總結: {passed} / {len(PHASE_7_CASES)} 通過 (通過率: {pct:.1f}%)")
    if failed == 0:
        print("🎉 Phase 7 完美全數通關！")
        return 0
    else:
        return 1

if __name__ == "__main__":
    sys.exit(run_tests())
