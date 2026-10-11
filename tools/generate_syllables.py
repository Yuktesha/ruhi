import sys
sys.stdout.reconfigure(encoding='utf-8')

with open(r'C:\ATGprjs\ruhi\ruhi_lexicon\data\lexicon_mcbopomofo.tsv', 'r', encoding='utf-8') as f:
    lines = f.readlines()

syls = set()
for line in lines:
    parts = line.strip().split('\t')
    if len(parts) >= 2:
        bpm_str = parts[1]
        if bpm_str == "BOPOMOFO":
            continue
        for token in bpm_str.split():
            clean = token.rstrip('ˊˇˋ˙ ')
            if clean and clean != "BOPOMOFO":
                syls.add(clean)

sorted_syls = sorted(list(syls))
with open(r'C:\ATGprjs\ruhi\ruhi-core\src\fsm\mandarin_syllables.rs', 'w', encoding='utf-8') as out:
    out.write('/// Valid Mandarin Chinese Base Syllables (430 valid syllables)\n')
    out.write('pub const VALID_MANDARIN_SYLLABLES: &[&str] = &[\n')
    for i in range(0, len(sorted_syls), 8):
        chunk = sorted_syls[i:i+8]
        items = ', '.join(f'"{s}"' for s in chunk)
        out.write(f'    {items},\n')
    out.write('];\n\n')
    out.write('pub fn is_valid_mandarin_syllable(bpm: &str) -> bool {\n')
    out.write('    VALID_MANDARIN_SYLLABLES.binary_search(&bpm).is_ok()\n')
    out.write('}\n')

print(f"Generated {len(sorted_syls)} syllables.")
