import ctypes, json, os, sys
sys.stdout.reconfigure(encoding="utf-8")

import sys
from pathlib import Path
_ROOT = Path(__file__).resolve().parent.parent
_TOOLS = _ROOT / "tools"
for p in [str(_ROOT), str(_TOOLS)]:
    if p not in sys.path:
        sys.path.insert(0, p)
lib = ctypes.CDLL(os.path.abspath("C:/ATGprjs/ruhi/ruhi-core/target/release/ruhi.dll"))
lib.ruhi_engine_create.restype = ctypes.c_void_p
lib.ruhi_engine_load_tsv.argtypes = [ctypes.c_void_p, ctypes.c_char_p]
lib.ruhi_engine_process_key.argtypes = [ctypes.c_void_p, ctypes.c_char]
lib.ruhi_engine_process_key.restype = ctypes.c_char_p
engine = lib.ruhi_engine_create()
lib.ruhi_engine_load_tsv(engine, "C:/ATGprjs/ruhi/ruhi_lexicon/data/lexicon_moe.tsv".encode("utf-8"))

# Type the full second half:
# 但我底下會照著我現在用自然打的
# 但(dnj) 我(xhf) 底(def) 下(verj) 會(hxij) 照(jwj) 著(jgs/js) 我(xhf) 現(venj) 在(zyj) 用(ulj) 自(zj) 然(rnd) 打(drf) 的(ds)
sentence_keys = "dnjxhfdefverjhxijjwjjsxhfvenjzyjuljzjrnddrfds"
output = ""
for k in sentence_keys:
    ptr = lib.ruhi_engine_process_key(engine, k.encode('ascii'))
    if ptr:
        res = json.loads(ctypes.cast(ptr, ctypes.c_char_p).value.decode('utf-8'))
        if res.get("action", "").startswith("commit"):
            output += res.get("committed_chunk", "")
        elif res.get("composed"):
            output = res.get("composed")

print("Full sentence output:")
print(output)
