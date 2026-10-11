import json
import os
import re
import sys
import pypinyin

INIT_MAP = {
    'ㄅ': 'b', 'ㄆ': 'p', 'ㄇ': 'm', 'ㄈ': 'f',
    'ㄉ': 'd', 'ㄊ': 't', 'ㄋ': 'n', 'ㄌ': 'l',
    'ㄍ': 'g', 'ㄎ': 'k', 'ㄏ': 'h',
    'ㄐ': 'j', 'ㄑ': 'v', 'ㄒ': 'c',
    'ㄓ': 'j', 'ㄔ': 'v', 'ㄕ': 'c',
    'ㄖ': 'r', 'ㄗ': 'z', 'ㄘ': 'a', 'ㄙ': 's',
}

MED_MAP = {
    'ㄧ': 'e', 'ㄨ': 'x', 'ㄩ': 'u',
}

FINAL_MAP = {
    'ㄚ': 'r', 'ㄛ': 'h', 'ㄜ': 'g', 'ㄝ': 'e',
    'ㄞ': 'i', 'ㄟ': 'a', 'ㄠ': 'w', 'ㄡ': 'o',
    'ㄢ': 'm', 'ㄣ': 'n', 'ㄤ': 'k', 'ㄥ': 'l', 'ㄦ': 'l',
}

TONE_MAP = {
    'ˊ': 'd',
    'ˇ': 'f',
    'ˋ': 'j',
    '˙': 's',
    '': ' ',
}

CONTRACTIONS = {
    'ㄉㄜ˙': 'ds',
    '˙ㄉㄜ': 'ds',
    'ㄇㄜ˙': 'ms',
    '˙ㄇㄜ': 'ms',
    'ㄌㄜ˙': 'ls',
    '˙ㄌㄜ': 'ls',
    'ㄋㄜ˙': 'ns',
    '˙ㄋㄜ': 'ns',
    'ㄓㄜ˙': 'js',
    '˙ㄓㄜ': 'js',
    'ㄅㄚ˙': 'bs',
    '˙ㄅㄚ': 'bs',
}

PUNCT_MAP = {
    '，': ',',
    '。': '.',
    '？': '?',
    '！': '!',
    '：': ':',
    '；': ';',
    '「': '[',
    '」': ']',
    '『': '{',
    '』': '}',
    '（': '(',
    '）': ')',
    ' ': ' ',
    '\n': '\n',
}

ZH_PATH = "C:/ATGprjs/Yuktesha.github.io/wordchain/zhuyin_map.json"
ZH_MAP = {}
if os.path.exists(ZH_PATH):
    with open(ZH_PATH, "r", encoding="utf-8") as f:
        ZH_MAP = json.load(f)

OVERRIDE_BPMF = {
    '什': 'ㄕㄣˊ',
    '麼': '˙ㄇㄜ',
    '的': '˙ㄉㄜ',
    '著': '˙ㄓㄜ',
    '地': '˙ㄉㄜ',
    '得': 'ㄉㄜˊ',
}

def parse_single_bopomofo(bpmf):
    if bpmf in CONTRACTIONS:
        return CONTRACTIONS[bpmf]

    tone = ''
    body = ''
    for ch in bpmf:
        if ch in ('ˊ', 'ˇ', 'ˋ', '˙'):
            tone = ch
        else:
            body += ch

    tone_key = TONE_MAP.get(tone, ' ')

    init_key = ''
    med_key = ''
    fin_key = ''

    i = 0
    if i < len(body) and body[i] in INIT_MAP:
        init_key = INIT_MAP[body[i]]
        i += 1

    if i < len(body) and body[i] in MED_MAP:
        med_key = MED_MAP[body[i]]
        i += 1

    if i < len(body) and body[i] in FINAL_MAP:
        fin_key = FINAL_MAP[body[i]]
        i += 1

    keys = init_key + med_key + fin_key
    if tone_key != ' ':
        keys += tone_key
    else:
        keys += ' '
    return keys

def text_to_hsu_keystrokes(text):
    result = []
    pinyin_list = pypinyin.pinyin(text, style=pypinyin.Style.BOPOMOFO, heteronym=False)
    
    for i, ch in enumerate(text):
        if ch in PUNCT_MAP:
            result.append(PUNCT_MAP[ch])
        elif ch in OVERRIDE_BPMF:
            bpmf = OVERRIDE_BPMF[ch]
            result.append(parse_single_bopomofo(bpmf))
        elif ch in ZH_MAP:
            bpmf = ZH_MAP[ch][0]
            result.append(parse_single_bopomofo(bpmf))
        elif i < len(pinyin_list) and pinyin_list[i]:
            bpmf = pinyin_list[i][0]
            if bpmf.startswith('˙'):
                bpmf = bpmf[1:] + '˙'
            result.append(parse_single_bopomofo(bpmf))
        else:
            result.append(ch)

    return ''.join(result)
