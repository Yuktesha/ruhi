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
ruhi_sandbox/test_phase_6.py
驗證 Phase 6: 介詞框式結構與處置/被動引力場 (Prepositional Framing & Disposal/Passive)
依據呂叔湘《現代漢語八百詞》介詞篇、劉月華《實用現代漢語語法》處置與被動式：
1. 處置式（把字句）：把事情做好、把作業寫完、把房間打掃乾淨、把杯子打破了、把書放回桌上、把問題解決掉、把心靜下來
2. 被動式（被字句）：被大家深深吸引、被困難打敗、被雨水淋濕、被熱情打動、被對手超越、被真誠感動
3. 框式介詞結構（在……之中/之下/之內/之外/心中）：在不知不覺之中、在大家共同努力之下、在有限的時間之內、在情理之外、在彼此的心中
4. 介引結構（對……來說/而言，從……開始，當……的時候）：對我來說非常重要、對大家而言是個好消息、從現在開始全力以赴、當陽光灑下的時候
"""
import os
import sys
import ctypes
import json
from pathlib import Path

sys.stdout.reconfigure(encoding='utf-8')

from test_phase_1 import get_hsu_keys_for_syllables

PHASE_6_CASES = [
    # ── 處置式（把字句） ──
    ("把事情做好", "ㄅㄚˇ ㄕˋ ㄑㄧㄥˊ ㄗㄨㄛˋ ㄏㄠˇ"),
    ("把作業寫完", "ㄅㄚˇ ㄗㄨㄛˋ ㄧㄝˋ ㄒㄧㄝˇ ㄨㄢˊ"),
    ("把房間打掃乾淨", "ㄅㄚˇ ㄈㄤˊ ㄐㄧㄢ ㄉㄚˇ ㄙㄠˇ ㄍㄢ ㄐㄧㄥˋ"),
    ("把杯子打破了", "ㄅㄚˇ ㄅㄟ ˙ㄗ ㄉㄚˇ ㄆㄛˋ ˙ㄌㄜ"),
    ("把書放回桌上", "ㄅㄚˇ ㄕㄨ ㄈㄤˋ ㄏㄨㄟˊ ㄓㄨㄛ ㄕㄤˋ"),
    ("把問題解決掉", "ㄅㄚˇ ㄨㄣˋ ㄊㄧˊ ㄐㄧㄝˇ ㄐㄩㄝˊ ㄉㄧㄠˋ"),
    ("把心靜下來", "ㄅㄚˇ ㄒㄧㄣ ㄐㄧㄥˋ ㄒㄧㄚˋ ㄌㄞˊ"),

    # ── 被動式（被字句） ──
    ("被大家深深吸引", "ㄅㄟˋ ㄉㄚˋ ㄐㄧㄚ ㄕㄣ ㄕㄣ ㄒㄧ ㄧㄣˇ"),
    ("被困難打敗", "ㄅㄟˋ ㄎㄨㄣˋ ㄋㄢˊ ㄉㄚˇ ㄅㄞˋ"),
    ("被雨水淋濕", "ㄅㄟˋ ㄩˇ ㄕㄨㄟˇ ㄌㄧㄣˊ ㄕ"),
    ("被熱情打動", "ㄅㄟˋ ㄖㄜˋ ㄑㄧㄥˊ ㄉㄚˇ ㄉㄨㄥˋ"),
    ("被對手超越", "ㄅㄟˋ ㄉㄨㄟˋ ㄕㄡˇ ㄔㄠ ㄩㄝˋ"),
    ("被真誠感動", "ㄅㄟˋ ㄓㄣ ㄔㄥˊ ㄍㄢˇ ㄉㄨㄥˋ"),

    # ── 框式介詞結構（在……之中/之下/之內/之外/心中） ──
    ("在不知不覺之中", "ㄗㄞˋ ㄅㄨˋ ㄓ ㄅㄨˋ ㄐㄩㄝˊ ㄓ ㄓㄨㄥ"),
    ("在大家共同努力之下", "ㄗㄞˋ ㄉㄚˋ ㄐㄧㄚ ㄍㄨㄥˋ ㄊㄨㄥˊ ㄋㄨˇ ㄌㄧˋ ㄓ ㄒㄧㄚˋ"),
    ("在有限的時間之內", "ㄗㄞˋ ㄧㄡˇ ㄒㄧㄢˋ ˙ㄉㄜ ㄕˊ ㄐㄧㄢ ㄓ ㄋㄟˋ"),
    ("在情理之外", "ㄗㄞˋ ㄑㄧㄥˊ ㄌㄧˇ ㄓ ㄨㄞˋ"),
    ("在彼此的心中", "ㄗㄞˋ ㄅㄧˇ ㄘˇ ˙ㄉㄜ ㄒㄧㄣ ㄓㄨㄥ"),

    # ── 介引結構（對……來說/而言，從……開始，當……的時候） ──
    ("對我來說非常重要", "ㄉㄨㄟˋ ㄨㄛˇ ㄌㄞˊ ㄕㄨㄛ ㄈㄟ ㄔㄤˊ ㄓㄨㄥˋ ㄧㄠˋ"),
    ("對大家而言是個好消息", "ㄉㄨㄟˋ ㄉㄚˋ ㄐㄧㄚ ㄦˊ ㄧㄢˊ ㄕˋ ㄍㄜˋ ㄏㄠˇ ㄒㄧㄠ ㄒㄧˊ"),
    ("從現在開始全力以赴", "ㄘㄨㄥˊ ㄒㄧㄢˋ ㄗㄞˋ ㄎㄞ ㄕˇ ㄑㄩㄢˊ ㄌㄧˋ ㄧˇ ㄈㄨˋ"),
    ("當陽光灑下的時候", "ㄉㄤ ㄧㄤˊ ㄍㄨㄤ ㄙㄚˇ ㄒㄧㄚˋ ˙ㄉㄜ ㄕˊ ㄏㄡˋ"),
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
    print("⛩️ Phase 6 (介詞框式結構與處置/被動引力場) 驗證開始")
    print("=" * 70)

    passed = 0
    failed = 0
    for expected, bpm_str in PHASE_6_CASES:
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
    pct = (passed / len(PHASE_6_CASES)) * 100.0
    print(f"總結: {passed} / {len(PHASE_6_CASES)} 通過 (通過率: {pct:.1f}%)")
    if failed == 0:
        print("🎉 Phase 6 完美全數通關！")
        return 0
    else:
        return 1

if __name__ == "__main__":
    sys.exit(run_tests())
