
import sys
from pathlib import Path
_ROOT = Path(__file__).resolve().parent.parent
_TOOLS = _ROOT / "tools"
for p in [str(_ROOT), str(_TOOLS)]:
    if p not in sys.path:
        sys.path.insert(0, p)
import ctypes
import json
import os
import sys

sys.stdout.reconfigure(encoding='utf-8')

def get_hsu_keys_for_syllables(syl_list):
    init_map = {
        'ㄅ': 'b', 'ㄆ': 'p', 'ㄇ': 'm', 'ㄈ': 'f',
        'ㄉ': 'd', 'ㄊ': 't', 'ㄋ': 'n', 'ㄌ': 'l',
        'ㄍ': 'g', 'ㄎ': 'k', 'ㄏ': 'h',
        'ㄐ': 'j', 'ㄑ': 'v', 'ㄒ': 'c',
        'ㄓ': 'j', 'ㄔ': 'v', 'ㄕ': 'c', 'ㄖ': 'r',
        'ㄗ': 'z', 'ㄘ': 'a', 'ㄙ': 's'
    }
    med_map = {'ㄧ': 'e', 'ㄨ': 'x', 'ㄩ': 'u'}
    fin_map = {
        'ㄚ': 'r', 'ㄛ': 'h', 'ㄜ': 'g', 'ㄝ': 'e',
        'ㄞ': 'i', 'ㄟ': 'a', 'ㄠ': 'w', 'ㄡ': 'o',
        'ㄢ': 'm', 'ㄣ': 'n', 'ㄤ': 'k', 'ㄥ': 'l', 'ㄦ': 'l'
    }

    keys = ''
    for s in syl_list:
        s = s.strip()
        if not s: continue
        tone = ' '
        if 'ˊ' in s: tone = 'd'; s = s.replace('ˊ', '')
        elif 'ˇ' in s: tone = 'f'; s = s.replace('ˇ', '')
        elif 'ˋ' in s: tone = 'j'; s = s.replace('ˋ', '')
        elif '˙' in s: tone = 's'; s = s.replace('˙', '')

        res = ''
        i = 0
        if len(s) > 0 and s[0] in init_map:
            res += init_map[s[0]]
            i += 1
        if i < len(s) and s[i] in med_map:
            res += med_map[s[i]]
            i += 1
        if i < len(s) and s[i] in fin_map:
            res += fin_map[s[i]]
            i += 1

        if tone == 's' and len(res) == 1 and res in 'dmnljb':
            keys += res + tone
        else:
            keys += res + tone
    return keys

TWISTERS = [
    # 1. 四和十
    (
        "四是四，十是十，十四是十四，四十是四十，別把十四說四十，別把四十說十四",
        "ㄙˋ ㄕˋ ㄙˋ ㄕˊ ㄕˋ ㄕˊ ㄕˊ ㄙˋ ㄕˋ ㄕˊ ㄙˋ ㄙˋ ㄕˊ ㄕˋ ㄙˋ ㄕˊ ㄅㄧㄝˊ ㄅㄚˇ ㄕˊ ㄙˋ ㄕㄨㄛ ㄙˋ ㄕˊ ㄅㄧㄝˊ ㄅㄚˇ ㄙˋ ㄕˊ ㄕㄨㄛ ㄕˊ ㄙˋ"
    ),
    # 2. 吃葡萄
    (
        "吃葡萄不吐葡萄皮，不吃葡萄倒吐葡萄皮",
        "ㄔ ㄆㄨˊ ㄊㄠˊ ㄅㄨˋ ㄊㄨˇ ㄆㄨˊ ㄊㄠˊ ㄆㄧˊ ㄅㄨˋ ㄔ ㄆㄨˊ ㄊㄠˊ ㄉㄠˋ ㄊㄨˇ ㄆㄨˊ ㄊㄠˊ ㄆㄧˊ"
    ),
    # 3. 扁擔與板凳
    (
        "扁擔寬，板凳長，扁擔想綁在板凳上，板凳不讓扁擔綁在板凳上",
        "ㄅㄧㄢˇ ㄉㄢˋ ㄎㄨㄢ ㄅㄢˇ ㄉㄥˋ ㄔㄤˊ ㄅㄧㄢˇ ㄉㄢˋ ㄒㄧㄤˇ ㄅㄤˇ ㄗㄞˋ ㄅㄢˇ ㄉㄥˋ ㄕㄤˋ ㄅㄢˇ ㄉㄥˋ ㄅㄨˊ ㄖㄤˋ ㄅㄧㄢˇ ㄉㄢˋ ㄅㄤˇ ㄗㄞˋ ㄅㄢˇ ㄉㄥˋ ㄕㄤˋ"
    ),
    # 4. 化肥會揮發
    (
        "黑化肥發黑會揮發，灰化肥發灰會揮黑",
        "ㄏㄟ ㄏㄨㄚˋ ㄈㄟˊ ㄈㄚ ㄏㄟ ㄏㄨㄟˋ ㄏㄨㄟ ㄈㄚ ㄏㄨㄟ ㄏㄨㄚˋ ㄈㄟˊ ㄈㄚ ㄏㄨㄟ ㄏㄨㄟˋ ㄏㄨㄟ ㄏㄟ"
    ),
    # 5. 喇嘛與啞巴
    (
        "南邊來了個喇嘛手裡提著五斤鰨蟆，北邊來了個啞巴腰裡別著個喇叭",
        "ㄋㄢˊ ㄅㄧㄢ ㄌㄞˊ ˙ㄌㄜ ㄍㄜ˙ ㄌㄚˇ ˙ㄇㄚ ㄕㄡˇ ㄌㄧˇ ㄊㄧˊ ˙ㄓㄜ ㄨˇ ㄐㄧㄣ ㄊㄚˇ ˙ㄇㄚ ㄅㄟˇ ㄅㄧㄢ ㄌㄞˊ ˙ㄌㄜ ㄍㄜ˙ ㄧㄚˇ ˙ㄅㄚ ㄧㄠ ㄌㄧˇ ㄅㄧㄝˊ ˙ㄓㄜ ㄍㄜ˙ ㄌㄚˇ ˙ㄅㄚ"
    ),
    # 6. 粉紅牆與鳳凰
    (
        "粉紅牆上畫鳳凰，鳳凰畫在粉紅牆",
        "ㄈㄣˇ ㄏㄨㄥˊ ㄑㄧㄤˊ ㄕㄤˋ ㄏㄨㄚˋ ㄈㄥˋ ㄏㄨㄤˊ ㄈㄥˋ ㄏㄨㄤˊ ㄏㄨㄚˋ ㄗㄞˋ ㄈㄣˇ ㄏㄨㄥˊ ㄑㄧㄤˊ"
    ),
    # 7. 和尚端湯
    (
        "和尚端湯上塔，塔滑湯灑湯燙塔",
        "ㄏㄜˊ ㄕㄤˋ ㄉㄨㄢ ㄊㄤ ㄕㄤˋ ㄊㄚˇ ㄊㄚˇ ㄏㄨㄚˊ ㄊㄤ ㄙㄚˇ ㄊㄤ ㄊㄤˋ ㄊㄚˇ"
    ),
    # 8. 漆匠與錫匠
    (
        "七巷一個漆匠，西巷一個錫匠，七巷漆匠用了西巷錫匠的錫杖",
        "ㄑㄧ ㄒㄧㄤˋ ㄧ ㄍㄜ˙ ㄑㄧ ㄐㄧㄤˋ ㄒㄧ ㄒㄧㄤˋ ㄧ ㄍㄜ˙ ㄒㄧˊ ㄐㄧㄤˋ ㄑㄧ ㄒㄧㄤˋ ㄑㄧ ㄐㄧㄤˋ ㄩㄥˋ ˙ㄌㄜ ㄒㄧ ㄒㄧㄤˋ ㄒㄧˊ ㄐㄧㄤˋ ㄉㄜ˙ ㄒㄧˊ ㄓㄤˋ"
    ),
]

def run_eval():
    dll_path = os.path.abspath("C:/ATGprjs/ruhi/ruhi-core/target/release/ruhi.dll")
    lib = ctypes.CDLL(dll_path)
    lib.ruhi_engine_create.restype = ctypes.c_void_p
    lib.ruhi_engine_load_tsv.argtypes = [ctypes.c_void_p, ctypes.c_char_p]
    lib.ruhi_engine_process_key.argtypes = [ctypes.c_void_p, ctypes.c_char]
    lib.ruhi_engine_process_key.restype = ctypes.c_char_p
    lib.ruhi_engine_reset.argtypes = [ctypes.c_void_p]

    engine = lib.ruhi_engine_create()
    tsv_path = "C:/ATGprjs/ruhi/ruhi_lexicon/data/lexicon_moe.tsv".encode("utf-8")
    lib.ruhi_engine_load_tsv(engine, tsv_path)

    print("================================================================================")
    print("  RuhI (如喜) 繞口令大師背書級 - 8 首經典繞口令評測")
    print("================================================================================\n")

    correct_count = 0
    for idx, (expected, bpm_str) in enumerate(TWISTERS, 1):
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

        clean_exp = expected.replace("？", "").replace("。", "").replace("，", "").replace("、", "")
        clean_out = output.replace("？", "").replace("。", "").replace("，", "").replace("、", "")

        is_match = (clean_exp == clean_out)
        if is_match:
            correct_count += 1
            status = "PASS"
        else:
            status = "FAIL"

        print(f"[{status}] 第 {idx:2d} 首:")
        print(f"  目標: {clean_exp}")
        print(f"  如喜: {clean_out}")
        if not is_match:
            diffs = []
            min_l = min(len(clean_exp), len(clean_out))
            for ci in range(min_l):
                if clean_exp[ci] != clean_out[ci]:
                    diffs.append(f"位{ci}: 應「{clean_exp[ci]}」實「{clean_out[ci]}」")
            if len(clean_exp) != len(clean_out):
                diffs.append(f"長度差異: 應 {len(clean_exp)} 字，實 {len(clean_out)} 字")
            print(f"  差異: {', '.join(diffs[:5])}")
        print()

    print(f"總結: {correct_count} / {len(TWISTERS)} 通過 (正確率: {correct_count/len(TWISTERS)*100:.1f}%)\n")

if __name__ == '__main__':
    run_eval()
