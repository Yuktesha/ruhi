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
ruhi_sandbox/test_phase_5.py
驗證 Phase 5: 複句關聯詞拓撲與語意配對引力場
涵蓋六大核心複句關係：
1. 讓步與遞進（即使...也、不但...而且、不僅...甚至）
2. 因果（因為...所以、由於...因此、既然...就）
3. 轉折（雖然...但是/可是/不過、儘管...還是）
4. 條件與假設（只要...就、只有...才、如果...就、除非...否則）
5. 選擇（與其...不如、寧可...也不）
6. 高頻代詞副詞複合體（我也要、過程、學歷、行程）
"""
import os
import sys
import ctypes
import json
from pathlib import Path

sys.stdout.reconfigure(encoding='utf-8')

from test_phase_1 import get_hsu_keys_for_syllables

PHASE_5_CASES = [
    # ── 讓步與遞進 ──
    ("即使明天下雨我也要去", "ㄐㄧˊ ㄕˇ ㄇㄧㄥˊ ㄊㄧㄢ ㄒㄧㄚˋ ㄩˇ ㄨㄛˇ ㄧㄝˇ ㄧㄠˋ ㄑㄩˋ"),
    ("不但學歷很高而且待人非常謙虛", "ㄅㄨˊ ㄉㄢˋ ㄒㄩㄝˊ ㄌㄧˋ ㄏㄣˇ ㄍㄠ ㄦˊ ㄑㄧㄝˇ ㄉㄞˋ ㄖㄣˊ ㄈㄟ ㄔㄤˊ ㄑㄧㄢ ㄒㄩ"),
    ("不僅能力出眾甚至超越前輩", "ㄅㄨˋ ㄐㄧㄣˇ ㄋㄥˊ ㄌㄧˋ ㄔㄨ ㄓㄨㄥˋ ㄕㄣˋ ㄓˋ ㄔㄠ ㄩㄝˋ ㄑㄧㄢˊ ㄅㄟˋ"),
    ("哪怕困難重重我也絕不放棄", "ㄋㄚˇ ㄆㄚˋ ㄎㄨㄣˋ ㄋㄢˊ ㄔㄨㄥˊ ㄔㄨㄥˊ ㄨㄛˇ ㄧㄝˇ ㄐㄩㄝˊ ㄅㄨˊ ㄈㄤˋ ㄑㄧˋ"),

    # ── 因果與轉折 ──
    ("因為天氣不好所以我們取消了行程", "ㄧㄣ ㄨㄟˋ ㄊㄧㄢ ㄑㄧˋ ㄅㄨˋ ㄏㄠˇ ㄙㄨㄛˇ ㄧˇ ㄨㄛˇ ˙ㄇㄣ ㄑㄩˇ ㄒㄧㄠ ˙ㄌㄜ ㄒㄧㄥˊ ㄔㄥˊ"),
    ("雖然過程很辛苦但是結果很甜美", "ㄙㄨㄟ ㄖㄢˊ ㄍㄨㄛˋ ㄔㄥˊ ㄏㄣˇ ㄒㄧㄣ ㄎㄨˇ ㄉㄢˋ ㄕˋ ㄐㄧㄝˊ ㄍㄨㄛˇ ㄏㄣˇ ㄊㄧㄢˊ ㄇㄟˇ"),
    ("既然來了就好好放鬆一下", "ㄐㄧˋ ㄖㄢˊ ㄌㄞˊ ˙ㄌㄜ ㄐㄧㄡˋ ㄏㄠˇ ㄏㄠˇ ㄈㄤˋ ㄙㄨㄥ ㄧ ㄒㄧㄚˋ"),
    ("儘管外界風風雨雨大家還是團結一心", "ㄐㄧㄣˇ ㄍㄨㄢˇ ㄨㄞˋ ㄐㄧㄝˋ ㄈㄥ ㄈㄥ ㄩˇ ㄩˇ ㄉㄚˋ ㄐㄧㄚ ㄏㄞˊ ㄕˋ ㄊㄨㄢˊ ㄐㄧㄝˊ ㄧ ㄒㄧㄣ"),

    # ── 條件與假設 ──
    ("只要堅持下去就會看到希望", "ㄓˇ ㄧㄠˋ ㄐㄧㄢ ㄔˊ ㄒㄧㄚˋ ㄑㄩˋ ㄐㄧㄡˋ ㄏㄨㄟˋ ㄎㄢˋ ㄉㄠˋ ㄒㄧ ㄨㄤˋ"),
    ("只有不斷努力才能獲得成功", "ㄓˇ ㄧㄡˇ ㄅㄨˊ ㄉㄨㄢˋ ㄋㄨˇ ㄌㄧˋ ㄘㄞˊ ㄋㄥˊ ㄏㄨㄛˋ ㄉㄜˊ ㄔㄥˊ ㄍㄨㄥ"),
    ("如果明天放假我們就去爬山", "ㄖㄨˊ ㄍㄨㄛˇ ㄇㄧㄥˊ ㄊㄧㄢ ㄈㄤˋ ㄐㄧㄚˋ ㄨㄛˇ ˙ㄇㄣ ㄐㄧㄡˋ ㄑㄩˋ ㄆㄚˊ ㄕㄢ"),
    ("除非親眼看見否則我絕不相信", "ㄔㄨˊ ㄈㄟ ㄑㄧㄣ ㄧㄢˇ ㄎㄢˋ ㄐㄧㄢˋ ㄈㄡˇ ㄗㄜˊ ㄨㄛˇ ㄐㄩㄝˊ ㄅㄨˋ ㄒㄧㄤ ㄒㄧㄣˋ"),

    # ── 選擇 ──
    ("與其坐以待斃不如奮力一搏", "ㄩˇ ㄑㄧˊ ㄗㄨㄛˋ ㄧˇ ㄉㄞˋ ㄅㄧˋ ㄅㄨˋ ㄖㄨˊ ㄈㄣˋ ㄌㄧˋ ㄧ ㄅㄛˊ"),
    ("寧可自己吃虧也不佔人便宜", "ㄋㄧㄥˋ ㄎㄜˇ ㄗˋ ㄐㄧˇ ㄔ ㄎㄨㄟ ㄧㄝˇ ㄅㄨˋ ㄓㄢˋ ㄖㄣˊ ㄆㄧㄢˊ ㄧˊ"),
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
    print("⛓️ Phase 5 (複句關聯詞拓撲與語意配對引力場) 驗證開始")
    print("=" * 70)

    passed = 0
    failed = 0
    for expected, bpm_str in PHASE_5_CASES:
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
    print(f"總結: {passed} / {len(PHASE_5_CASES)} 通過 (通過率: {passed / len(PHASE_5_CASES) * 100:.1f}%)")
    return failed == 0

if __name__ == "__main__":
    success = run_tests()
    sys.exit(0 if success else 1)
