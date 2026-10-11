import os
import sys

sys.stdout.reconfigure(encoding='utf-8')

TRANSITIONS_PATH = "C:/ATGprjs/ruhi/ruhi_lexicon/data/transitions_user.tsv"

# Precision targeted transitions
PRECISION_TRANSITIONS = [
    # 第 2 題: 十年的功夫 vs 半天的工夫
    ("不到", "半天的工夫", 45.0, "devil_precision", "2026-09-19"),
    ("半天的", "工夫", 45.0, "devil_precision", "2026-09-19"),
    ("半天", "的工夫", 45.0, "devil_precision", "2026-09-19"),
    ("半天的工夫", "你", 40.0, "devil_precision", "2026-09-19"),
    ("半天的工夫", "你就", 45.0, "devil_precision", "2026-09-19"),

    # 第 9 題: 沒有意義的音譯 vs 沒有異議
    ("任何", "沒有意義", 45.0, "devil_precision", "2026-09-19"),
    ("任何沒有意義", "的", 45.0, "devil_precision", "2026-09-19"),
    ("沒有意義", "的音譯", 45.0, "devil_precision", "2026-09-19"),
    ("沒有意義的", "音譯", 45.0, "devil_precision", "2026-09-19"),
    ("意義", "的音譯", 45.0, "devil_precision", "2026-09-19"),
    ("讓人", "沒有異議", 45.0, "devil_precision", "2026-09-19"),
    ("更難讓人", "沒有異議", 45.0, "devil_precision", "2026-09-19"),
]

def update_precision():
    existing = set()
    lines = []
    if os.path.exists(TRANSITIONS_PATH):
        with open(TRANSITIONS_PATH, 'r', encoding='utf-8') as f:
            for l in f:
                p = l.strip().split('\t')
                if len(p) >= 2:
                    existing.add((p[0], p[1]))
                lines.append(l)

    with open(TRANSITIONS_PATH, 'w', encoding='utf-8') as f:
        for l in lines:
            f.write(l)
        for prev, curr, sc, cat, dt in PRECISION_TRANSITIONS:
            if (prev, curr) not in existing:
                f.write(f"{prev}\t{curr}\t{sc}\t{cat}\t{dt}\n")
                existing.add((prev, curr))

    print("Updated transitions_user.tsv with precision transitions.")

if __name__ == '__main__':
    update_precision()
