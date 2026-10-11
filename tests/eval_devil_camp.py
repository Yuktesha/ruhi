
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

# Predefined exact pronunciations for the 13 stress sentences
SENTENCES = [
    # 1. 報導 vs 報到
    (
        "你各位新來的記者們到我們美華報導報到是為了要報導我們美華報導的報到情況嗎？",
        "ㄋㄧˇ ㄍㄜˋ ㄨㄟˋ ㄒㄧㄣ ㄌㄞˊ ㄉㄜ˙ ㄐㄧˋ ㄓㄜˇ ˙ㄇㄣ ㄉㄠˋ ㄨㄛˇ ˙ㄇㄣ ㄇㄟˇ ㄏㄨㄚˊ ㄅㄠˋ ㄉㄠˋ ㄅㄠˋ ㄉㄠˋ ㄕˋ ㄨㄟˋ ˙ㄌㄜ ㄧㄠˋ ㄅㄠˋ ㄉㄠˋ ㄨㄛˇ ˙ㄇㄣ ㄇㄟˇ ㄏㄨㄚˊ ㄅㄠˋ ㄉㄠˋ ㄉㄜ˙ ㄅㄠˋ ㄉㄠˋ ㄑㄧㄥˊ ㄎㄨㄤˋ ˙ㄇㄚ"
    ),
    # 2. 功夫 vs 工夫
    (
        "別人練了十年的功夫竟然不到半天的工夫你就全然得來全不費工夫的得到了。",
        "ㄅㄧㄝˊ ㄖㄣˊ ㄌㄧㄢˋ ˙ㄌㄜ ㄕˊ ㄋㄧㄢˊ ㄉㄜ˙ ㄍㄨㄥ ㄈㄨ ㄐㄧㄥˋ ㄖㄢˊ ㄅㄨˋ ㄉㄠˋ ㄅㄢˋ ㄊㄧㄢ ㄉㄜ˙ ㄍㄨㄥ ㄈㄨ ㄋㄧˇ ㄐㄧㄡˋ ㄑㄩㄢˊ ㄖㄢˊ ㄉㄜˊ ㄌㄞˊ ㄑㄩㄢˊ ㄅㄨˊ ㄈㄟˋ ㄍㄨㄥ ㄈㄨ ㄉㄜ˙ ㄉㄜˊ ㄉㄠˋ ˙ㄌㄜ"
    ),
    # 3. 權利 vs 全力 vs 權力
    (
        "為了爭取我們應有的權利我們必須全力爭取權力才使得上力。",
        "ㄨㄟˋ ˙ㄌㄜ ㄓㄥ ㄑㄩˇ ㄨㄛˇ ˙ㄇㄣ ㄧㄥ ㄧㄡˇ ㄉㄜ˙ ㄑㄩㄢˊ ㄌㄧˋ ㄨㄛˇ ˙ㄇㄣ ㄅㄧˋ ㄒㄩ ㄑㄩㄢˊ ㄌㄧˋ ㄓㄥ ㄑㄩˇ ㄑㄩㄢˊ ㄌㄧˋ ㄘㄞˊ ㄕˇ ㄉㄜ˙ ㄕㄤˋ ㄌㄧˋ"
    ),
    # 4. 集會 vs 忌諱 vs 機會
    (
        "在集會中最忌諱機會主義者講一些可以在某些集會裡以可笑的忌諱得到什麼機會。",
        "ㄗㄞˋ ㄐㄧˊ ㄏㄨㄟˋ ㄓㄨㄥ ㄗㄨㄟˋ ㄐㄧˋ ㄏㄨㄟˋ ㄐㄧ ㄏㄨㄟˋ ㄓㄨˇ ㄧˋ ㄓㄜˇ ㄐㄧㄤˇ ㄧ ㄒㄧㄝ ㄎㄜˇ ㄧˇ ㄗㄞˋ ㄇㄡˇ ㄒㄧㄝ ㄐㄧˊ ㄏㄨㄟˋ ㄌㄧˇ ㄧˇ ㄎㄜˇ ㄒㄧㄠˋ ㄉㄜ˙ ㄐㄧˋ ㄏㄨㄟˋ ㄉㄜˊ ㄉㄠˋ ㄕㄣˊ ˙ㄇㄜ ㄐㄧ ㄏㄨㄟˋ"
    ),
    # 5. 精力 vs 經歷
    (
        "總是充滿精力的他的經歷果然顯示他無所不在的精力表現。",
        "ㄗㄨㄥˇ ㄕˋ ㄔㄨㄥ ㄇㄢˇ ㄐㄧㄥ ㄌㄧˋ ㄉㄜ˙ ㄊㄚ ㄉㄜ˙ ㄐㄧㄥ ㄌㄧˋ ㄍㄨㄛˇ ㄖㄢˊ ㄒㄧㄢˇ ㄕˋ ㄊㄚ ㄨˊ ㄙㄨㄛˇ ㄅㄨˋ ㄗㄞˋ ㄉㄜ˙ ㄐㄧㄥ ㄌㄧˋ ㄅㄧㄠˇ ㄒㄧㄢˋ"
    ),
    # 6. 反應 vs 反映
    (
        "這些人的反應反映了真實的反應。",
        "ㄓㄜˋ ㄒㄧㄝ ㄖㄣˊ ㄉㄜ˙ ㄈㄢˇ ㄧㄥˋ ㄈㄢˇ ㄧㄥˋ ˙ㄌㄜ ㄓㄣ ㄕˊ ㄉㄜ˙ ㄈㄢˇ ㄧㄥˋ"
    ),
    # 7. 自燃 vs 自然
    (
        "人體自燃並不自然。",
        "ㄖㄣˊ ㄊㄧˇ ㄗˋ ㄖㄢˊ ㄅㄧㄥˋ ㄅㄨˋ ㄗˋ ㄖㄢˊ"
    ),
    # 8. 衷心 vs 中心 vs 忠心
    (
        "我衷心期望您不要在我們中心只是成為一條忠心的狗。",
        "ㄨㄛˇ ㄓㄨㄥ ㄒㄧㄣ ㄑㄧˊ ㄨㄤˋ ㄋㄧㄣˊ ㄅㄨˊ ㄧㄠˋ ㄗㄞˋ ㄨㄛˇ ˙ㄇㄣ ㄓㄨㄥ ㄒㄧㄣ ㄓˇ ㄕˋ ㄔㄥˊ ㄨㄟˊ ㄧ ㄊㄧㄠˊ ㄓㄨㄥ ㄒㄧㄣ ㄉㄜ˙ ㄍㄡˇ"
    ),
    # 9. 意義 vs 音譯 vs 意譯 vs 異議
    (
        "任何沒有意義的音譯甚至比詞不達意的意譯更難讓人沒有異議。",
        "ㄖㄣˋ ㄏㄜˊ ㄇㄟˊ ㄧㄡˇ ㄧˋ ㄧˋ ㄉㄜ˙ ㄧㄣ ㄧˋ ㄕㄣˋ ㄓˋ ㄅㄧˇ ㄘˊ ㄅㄨˋ ㄉㄚˊ ㄧˋ ㄉㄜ˙ ㄧˋ ㄧˋ ㄍㄥˋ ㄋㄢˊ ㄖㄤˋ ㄖㄣˊ ㄇㄟˊ ㄧㄡˇ ㄧˋ ㄧˋ"
    ),
    # 10. 試試 vs 事事 vs 世事 vs 逝世
    (
        "試試看一個人真的能夠事事如意的面對世事直到逝世嗎？",
        "ㄕˋ ㄕˋ ㄎㄢˋ ㄧ ㄍㄜ˙ ㄖㄣˊ ㄓㄣ ㄉㄜ˙ ㄋㄥˊ ㄍㄡˋ ㄕˋ ㄕˋ ㄖㄨˊ ㄧˋ ㄉㄜ˙ ㄇㄧㄢˋ ㄉㄨㄟˋ ㄕˋ ㄕˋ ㄓˊ ㄉㄠˋ ㄕˋ ㄕˋ ˙ㄇㄚ"
    ),
    # 11. 付單 vs 負擔
    (
        "學人家搶著付單完全超出我的負擔。",
        "ㄒㄩㄝˊ ㄖㄣˊ ㄐㄧㄚ ㄑㄧㄤˇ ˙ㄓㄜ ㄈㄨˋ ㄉㄢ ㄨㄢˊ ㄑㄩㄢˊ ㄔㄠ ㄔㄨ ㄨㄛˇ ㄉㄜ˙ ㄈㄨˋ ㄉㄢ"
    ),
    # 12. 美到忘記 vs 每到 vs 旺季 vs 記過
    (
        "這位號稱美到忘記的美人每到美麗的旺季從來沒有真的美到忘記過，倒是經常害人記過。",
        "ㄓㄜˋ ㄨㄟˋ ㄏㄠˋ ㄔㄥ ㄇㄟˇ ㄉㄠˋ ㄨㄤˋ ㄐㄧˋ ㄉㄜ˙ ㄇㄟˇ ㄖㄣˊ ㄇㄟˇ ㄉㄠˋ ㄇㄟˇ ㄌㄧˋ ㄉㄜ˙ ㄨㄤˋ ㄐㄧˋ ㄘㄨㄥˊ ㄌㄞˊ ㄇㄟˊ ㄧㄡˇ ㄓㄣ ㄉㄜ˙ ㄇㄟˇ ㄉㄠˋ ㄨㄤˋ ㄐㄧˋ ㄍㄨㄛˋ ㄉㄠˋ ㄕˋ ㄐㄧㄥ ㄔㄤˊ ㄏㄞˋ ㄖㄣˊ ㄐㄧˋ ㄍㄨㄛˋ"
    ),
    # 13. 首善之都 vs 首都 vs 以他為首 vs 都獲得
    (
        "在這個首善之都的首都凡人以他為首都獲得美好的成就。",
        "ㄗㄞˋ ㄓㄜˋ ㄍㄜ˙ ㄕㄡˇ ㄕㄢˋ ㄓ ㄉㄨ ㄉㄜ˙ ㄕㄡˇ ㄉㄨ ㄈㄢˊ ㄖㄣˊ ㄧˇ ㄊㄚ ㄨㄟˊ ㄕㄡˇ ㄉㄡ ㄏㄨㄛˋ ㄉㄜˊ ㄇㄟˇ ㄏㄠˇ ㄉㄜ˙ ㄔㄥˊ ㄐㄧㄡˋ"
    ),
    # 14. 治癒 vs 不至於 & 設計 vs 社稷 vs 射祭 & 圖案 vs 圖暗 & 倒是 vs 道是 (神級多重同音大亂鬥)
    (
        "這款旨在治癒人心的圖案設計，究竟是為了拯救社稷的宏偉設計，抑或是為了不至於淪為射祭圖暗設計，倒是道是設計很簡單？",
        "ㄓㄜˋ ㄎㄨㄢˇ ㄓˇ ㄗㄞˋ ㄓˋ ㄩˋ ㄖㄣˊ ㄒㄧㄣ ㄉㄜ˙ ㄊㄨˊ ㄢˋ ㄕㄜˋ ㄐㄧˋ ㄐㄧㄡˋ ㄐㄧㄥˋ ㄕˋ ㄨㄟˋ ˙ㄌㄜ ㄓㄥˇ ㄐㄧㄡˋ ㄕㄜˋ ㄐㄧˋ ㄉㄜ˙ ㄏㄨㄥˊ ㄨㄟˇ ㄕㄜˋ ㄐㄧˋ ㄧˋ ㄏㄨㄛˋ ㄕˋ ㄨㄟˋ ˙ㄌㄜ ㄅㄨˊ ㄓˋ ㄩˊ ㄌㄨㄣˊ ㄨㄟˊ ㄕㄜˋ ㄐㄧˋ ㄊㄨˊ ㄢˋ ㄕㄜˋ ㄐㄧˋ ㄉㄠˋ ㄕˋ ㄉㄠˋ ㄕˋ ㄕㄜˋ ㄐㄧˋ ㄏㄣˇ ㄐㄧㄢˇ ㄉㄢ"
    ),
    # 15. 資料庫 vs 資料褲 vs 資料酷 vs 資料哭 (同音四態雙關與長程語意場極限考驗)
    (
        "作為一名資料庫建置專家，把資料做成一件資料褲來穿耍一下資料酷、杜絕資料哭，應該是天經地義的事。",
        "ㄗㄨㄛˋ ㄨㄟˊ ㄧ ㄇㄧㄥˊ ㄗ ㄌㄧㄠˋ ㄎㄨˋ ㄐㄧㄢˋ ㄓˋ ㄓㄨㄢ ㄐㄧㄚ ㄅㄚˇ ㄗ ㄌㄧㄠˋ ㄗㄨㄛˋ ㄔㄥˊ ㄧ ㄐㄧㄢˋ ㄗ ㄌㄧㄠˋ ㄎㄨˋ ㄌㄞˊ ㄔㄨㄢ ㄕㄨㄚˇ ㄧ ㄒㄧㄚˋ ㄗ ㄌㄧㄠˋ ㄎㄨˋ ㄉㄨˋ ㄐㄩㄝˊ ㄗ ㄌㄧㄠˋ ㄎㄨ ㄧㄥ ㄍㄞ ㄕˋ ㄊㄧㄢ ㄐㄧㄥ ㄉㄧˋ ㄧˋ ㄉㄜ˙ ㄕˋ"
    ),
    # 16. 全隊全對 vs 全隊也全對 & 等下依據 vs 等下一句 (四重同音與跨詞邊界交錯歧義)
    (
        "這句語文測驗簡單到全隊全對不稀奇，等下依據如喜魔鬼測試的每一句全隊也全對才離奇。",
        "ㄓㄜˋ ㄐㄩˋ ㄩˇ ㄨㄣˊ ㄘㄜˋ ㄧㄢˋ ㄐㄧㄢˇ ㄉㄢ ㄉㄠˋ ㄑㄩㄢˊ ㄉㄨㄟˋ ㄑㄩㄢˊ ㄉㄨㄟˋ ㄅㄨˋ ㄒㄧ ㄑㄧˊ ㄉㄥˇ ㄒㄧㄚˋ ㄧ ㄐㄩˋ ㄖㄨˊ ㄒㄧˇ ㄇㄛˊ ㄍㄨㄟˇ ㄘㄜˋ ㄕˋ ㄉㄜ˙ ㄇㄟˇ ㄧ ㄐㄩˋ ㄑㄩㄢˊ ㄉㄨㄟˋ ㄧㄝˇ ㄑㄩㄢˊ ㄉㄨㄟˋ ㄘㄞˊ ㄌㄧˊ ㄑㄧˊ"
    ),
]

def run_eval():
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

    # 載入 Tier 2 領域詞庫
    dom_path = "C:/ATGprjs/ruhi/ruhi_lexicon/data/lexicon_domain.tsv".encode("utf-8")
    if os.path.exists("C:/ATGprjs/ruhi/ruhi_lexicon/data/lexicon_domain.tsv"):
        lib.ruhi_engine_load_tsv(engine, dom_path)

    # 載入語意轉移矩陣
    trans_path = "C:/ATGprjs/ruhi/ruhi_lexicon/data/transitions_user.tsv".encode("utf-8")
    if os.path.exists("C:/ATGprjs/ruhi/ruhi_lexicon/data/transitions_user.tsv"):
        lib.ruhi_engine_load_transitions_tsv(engine, trans_path)

    print("================================================================================")
    print("  RuhI (如喜) 魔鬼訓練營 16 道極限語意測試題評測")
    print("================================================================================\n")

    correct_count = 0
    for idx, (expected, bpm_str) in enumerate(SENTENCES, 1):
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

        # Clean punctuation from expected for comparison
        clean_exp = expected.replace("？", "").replace("。", "").replace("，", "").replace("、", "")
        clean_out = output.replace("？", "").replace("。", "").replace("，", "").replace("、", "")

        is_match = (clean_exp == clean_out)
        if is_match:
            correct_count += 1
            status = "PASS"
        else:
            status = "FAIL"

        print(f"[{status}] 第 {idx:2d} 題:")
        print(f"  目標: {clean_exp}")
        print(f"  如喜: {clean_out}")
        if not is_match:
            # show diff
            diffs = []
            min_l = min(len(clean_exp), len(clean_out))
            for ci in range(min_l):
                if clean_exp[ci] != clean_out[ci]:
                    diffs.append(f"位{ci}: 應為「{clean_exp[ci]}」但為「{clean_out[ci]}」")
            if len(clean_exp) != len(clean_out):
                diffs.append(f"長度差異: 應為 {len(clean_exp)} 字，但為 {len(clean_out)} 字")
            print(f"  差異: {', '.join(diffs[:5])}")
        print()

    print(f"總結: {correct_count} / {len(SENTENCES)} 通過 (正確率: {correct_count/len(SENTENCES)*100:.1f}%)\n")

if __name__ == '__main__':
    run_eval()
