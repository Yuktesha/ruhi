import os
import sys

sys.stdout.reconfigure(encoding='utf-8')

SYNC_SCRIPT = "C:/ATGprjs/ruhi/tools/sync_wordchain_to_ruhi.py"

with open(SYNC_SCRIPT, "r", encoding="utf-8") as f:
    content = f.read()

# Replace any occurrence of ㄉㄜ˙ with ˙ㄉㄜ, ㄌㄜ˙ with ˙ㄌㄜ, ㄇㄚ˙ with ˙ㄇㄚ, ㄇㄣ˙ with ˙ㄇㄣ, ㄓㄜ˙ with ˙ㄓㄜ
replacements = [
    ("ㄉㄜ˙", "˙ㄉㄜ"),
    ("ㄌㄜ˙", "˙ㄌㄜ"),
    ("ㄇㄚ˙", "˙ㄇㄚ"),
    ("ㄇㄣ˙", "˙ㄇㄣ"),
    ("ㄓㄜ˙", "˙ㄓㄜ"),
    ("ㄒㄧㄚ˙", "˙ㄒㄧㄚ"),
]

for old_s, new_s in replacements:
    content = content.replace(old_s, new_s)

with open(SYNC_SCRIPT, "w", encoding="utf-8") as f:
    f.write(content)

print("Standardized all light tones to prefix form in sync_wordchain_to_ruhi.py")
