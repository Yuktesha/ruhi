#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
字戀 (WordChain) ⇄ 如喜 (RuhI)
建立與豐富化共通擴充知識庫 (supplementary_lexicon.json)
將魔鬼訓練營、經典繞口令與高頻現代口語轉換為符合字戀遊戲故事卡與如喜權重的結構
"""

import json
import os
import sys
import pypinyin

sys.stdout.reconfigure(encoding='utf-8')

WORDCHAIN_DIR = os.path.abspath("C:/ATGprjs/Yuktesha.github.io/wordchain")
SUPPLEMENTARY_PATH = os.path.join(WORDCHAIN_DIR, "supplementary_lexicon.json")

ENTRIES = [
    # 1. 傳統棋諺與民間熟語
    {
        "word": "起手無回",
        "category": "saying",
        "origin": "傳統象棋棋諺",
        "def": "傳統棋藝諺語：「起手無回大丈夫，落子無悔大丈夫」。形容下棋落子不悔，比喻行事果決、說話算話、絕不反悔。",
        "usage": "「棋局如此，人生亦然，起手無回大丈夫。」",
        "ruhi_weight": 950,
        "transitions": [["起手", "無回", 35.0]]
    },
    {
        "word": "白白犧牲",
        "category": "idiom",
        "origin": "現代通俗熟語",
        "def": "徒然無益地付出生命、代價或心血，而未能達成預期目的或產生正面價值。",
        "usage": "「我們絕不能讓先烈的心血白白犧牲。」",
        "ruhi_weight": 950,
        "transitions": [["白白", "犧牲", 35.0]]
    },

    # 2. 魔鬼訓練營 13 題核心詞與長成語
    {
        "word": "得來全不費工夫",
        "category": "idiom",
        "origin": "《初刻拍案驚奇》",
        "def": "比喻毫不費力氣就獲得了夢寐以求的東西。語出宋代佚名詩：「踏破鐵鞋無覓處，得來全不費工夫。」",
        "usage": "「真是踏破鐵鞋無覓處，得來全不費工夫！」",
        "ruhi_weight": 995,
        "transitions": [["全然", "得來全不費工夫", 40.0]]
    },
    {
        "word": "全不費工夫",
        "category": "idiom",
        "origin": "宋代通俗詩語",
        "def": "毫無費力、毫不費事之意。",
        "usage": "「這樁差事辦得順利，全不費工夫。」",
        "ruhi_weight": 995,
        "transitions": [["全然", "全不費工夫", 40.0]]
    },
    {
        "word": "美華報導",
        "category": "modern",
        "origin": "新聞傳播機構",
        "def": "知名新聞傳媒報導機構之全名，代表客觀翔實之新聞採訪現場。",
        "usage": "「歡迎各位新來的記者到我們美華報導報到。」",
        "ruhi_weight": 980,
        "transitions": [["美華", "報導", 35.0], ["報導", "報到", 30.0]]
    },
    {
        "word": "十年的功夫",
        "category": "idiom",
        "origin": "傳統武藝諺語",
        "def": "指經歷漫長歲月苦心磨礪方能練就的精深技藝與深厚功底。",
        "usage": "「別人練了十年的功夫，豈是朝夕可至。」",
        "ruhi_weight": 990,
        "transitions": [["練了", "十年", 25.0], ["十年的", "功夫", 35.0]]
    },
    {
        "word": "半天的工夫",
        "category": "modern",
        "origin": "通俗口語熟語",
        "def": "極言時間短暫，不到半日的時間或心力。",
        "usage": "「想不到不到半天的工夫你就全學會了。」",
        "ruhi_weight": 990,
        "transitions": [["不到", "半天", 25.0], ["半天的", "工夫", 35.0]]
    },
    {
        "word": "使得上力",
        "category": "modern",
        "origin": "民間行話口語",
        "def": "能夠發揮出力量或實質作用，能派上用場、施展拳腳。",
        "usage": "「唯有團結一致，爭取權力，才使得上力。」",
        "ruhi_weight": 950,
        "transitions": [["才", "使得上力", 35.0]]
    },
    {
        "word": "機會主義者",
        "category": "modern",
        "origin": "現代政治學術語",
        "def": "指缺乏堅定原則，專門察言觀色、投機取巧以謀取私利之人。",
        "usage": "「在集會中最忌諱那些見風轉舵的機會主義者。」",
        "ruhi_weight": 980,
        "transitions": [["忌諱", "機會主義者", 35.0]]
    },
    {
        "word": "充滿精力",
        "category": "modern",
        "origin": "心理學與活力修辭",
        "def": "精神飽滿、活力充沛，展現源源不絕的行動力。",
        "usage": "「總是充滿精力的他，工作起來不知疲倦。」",
        "ruhi_weight": 980,
        "transitions": [["充滿", "精力", 35.0]]
    },
    {
        "word": "他的經歷",
        "category": "modern",
        "origin": "人物傳記短語",
        "def": "某人過往親身走過的人生閱歷與經驗。",
        "usage": "「他的經歷果然顯示出無所不在的領導才幹。」",
        "ruhi_weight": 980,
        "transitions": [["他的", "經歷", 35.0]]
    },
    {
        "word": "人體自燃",
        "category": "modern",
        "origin": "科學超自然名詞",
        "def": "指人體在未接觸外來火源的情況下，自行起火燃燒的神秘物理現象。",
        "usage": "「科學界認為人體自燃並不自然。」",
        "ruhi_weight": 980,
        "transitions": [["人體", "自燃", 40.0], ["自燃", "並不", 30.0], ["並不", "自然", 40.0]]
    },
    {
        "word": "忠心的狗",
        "category": "saying",
        "origin": "寓言文學比喻",
        "def": "比喻盲目效忠或毫無自主意志的效忠者。",
        "usage": "「不要在我們中心只是成為一條忠心的狗。」",
        "ruhi_weight": 980,
        "transitions": [["忠心的", "狗", 40.0]]
    },
    {
        "word": "詞不達意",
        "category": "idiom",
        "origin": "《儀禮·聘禮》",
        "def": "詞句無法切實表達出內心真正想要抒發的意思。語本《論語·衛靈公》：「辭達而已矣。」",
        "usage": "「詞不達意的意譯，比沒有意義的音譯更難讓人理解。」",
        "ruhi_weight": 980,
        "transitions": [["詞不達意的", "意譯", 40.0]]
    },
    {
        "word": "事事如意",
        "category": "idiom",
        "origin": "傳統歲時吉祥成語",
        "def": "萬事稱心、吉祥如願。祝賀他人事事順遂之吉祥語。",
        "usage": "「誰能真的事事如意的面對世事直到逝世？」",
        "ruhi_weight": 980,
        "transitions": [["事事", "如意", 35.0], ["面對", "世事", 40.0], ["直到", "逝世", 40.0]]
    },
    {
        "word": "美到忘記",
        "category": "modern",
        "origin": "極致讚嘆修辭",
        "def": "形容容顏或風景美艷絕倫，令人神魂顛倒、忘卻俗事煩憂。",
        "usage": "「這位號稱美到忘記的美人，每到旺季都吸引無數人潮。」",
        "ruhi_weight": 980,
        "transitions": [["號稱", "美到忘記", 40.0], ["美到忘記", "的美人", 40.0]]
    },
    {
        "word": "首善之都",
        "category": "idiom",
        "origin": "《漢書·儒林傳》",
        "def": "指全國道德、文化與政教風尚的典範首都。",
        "usage": "「在這個首善之都的首都，凡人亦可成就非凡。」",
        "ruhi_weight": 980,
        "transitions": [["首善之都", "的", 25.0], ["的", "首都", 35.0], ["以他為首", "都獲得", 40.0]]
    },
    {
        "word": "盆滿缽溢",
        "category": "idiom",
        "origin": "民間商賈熟語",
        "def": "形容賺錢極多、收穫豐富，資財滿溢而出。",
        "usage": "「每到旺季大家都賺到盆滿缽溢。」",
        "ruhi_weight": 980,
        "transitions": [["賺到", "盆滿缽溢", 35.0]]
    },
    {
        "word": "為之瘋狂",
        "category": "idiom",
        "origin": "現代激賞熟語",
        "def": "受某種狂熱氣氛或人事物所吸引，到了近乎痴狂的熱烈程度。",
        "usage": "「這場精彩的表演讓在場所有人為之瘋狂。」",
        "ruhi_weight": 980,
        "transitions": [["為之", "瘋狂", 35.0]]
    },

    # 3. 經典 8 首繞口令名句與專有名詞
    {
        "word": "四是四",
        "category": "saying",
        "origin": "傳統相聲繞口令《四和十》",
        "def": "強調名實相符、是非分明之經典說唱繞口令開篇。",
        "usage": "「四是四，十是十，十四是十四，四十是四十。」",
        "ruhi_weight": 980,
        "transitions": [["四是四", "十是十", 30.0]]
    },
    {
        "word": "十是十",
        "category": "saying",
        "origin": "傳統相聲繞口令《四和十》",
        "def": "強調音節清晰、分清平舌與翹舌音之對仗短語。",
        "usage": "「十是十，十四是十四，四十是四十。」",
        "ruhi_weight": 980,
        "transitions": [["十是十", "十四是十四", 30.0]]
    },
    {
        "word": "十四是十四",
        "category": "saying",
        "origin": "傳統相聲繞口令《四和十》",
        "def": "經典繞口令對稱句型，考驗舌尖發音分寸。",
        "usage": "「十四是十四，四十是四十，別把十四說四十。」",
        "ruhi_weight": 995,
        "transitions": [["十四是十四", "四十是四十", 30.0]]
    },
    {
        "word": "四十是四十",
        "category": "saying",
        "origin": "傳統相聲繞口令《四和十》",
        "def": "經典繞口令平翹舌轉換極限句。",
        "usage": "「四十是四十，別把十四說四十，別把四十說十四。」",
        "ruhi_weight": 995,
        "transitions": [["四十是四十", "別把", 30.0], ["別把", "十四", 35.0], ["別把", "四十", 35.0]]
    },
    {
        "word": "吃葡萄不吐葡萄皮",
        "category": "saying",
        "origin": "民間相聲繞口令《吃葡萄》",
        "def": "家喻戶曉之經典雙唇爆破音繞口令名句。",
        "usage": "「吃葡萄不吐葡萄皮，不吃葡萄倒吐葡萄皮。」",
        "ruhi_weight": 995,
        "transitions": [["吃葡萄", "不吐葡萄皮", 40.0], ["葡萄皮", "不吃葡萄", 35.0]]
    },
    {
        "word": "不吃葡萄倒吐葡萄皮",
        "category": "saying",
        "origin": "民間相聲繞口令《吃葡萄》",
        "def": "諷刺荒謬反常舉止之滑稽繞口令下聯。",
        "usage": "「不吃葡萄倒吐葡萄皮，逗得全場哈哈大笑。」",
        "ruhi_weight": 995,
        "transitions": [["不吃葡萄", "倒吐葡萄皮", 40.0]]
    },
    {
        "word": "扁擔寬板凳長",
        "category": "saying",
        "origin": "北方相聲繞口令《扁擔與板凳》",
        "def": "描摹器物特徵之經典舌音繞口令起首句。",
        "usage": "「扁擔寬，板凳長，扁擔想綁在板凳上。」",
        "ruhi_weight": 980,
        "transitions": [["扁擔", "寬", 30.0], ["板凳", "長", 30.0]]
    },
    {
        "word": "黑化肥發黑會揮發",
        "category": "saying",
        "origin": "經典唇齒音相聲繞口令《化肥會揮發》",
        "def": "極限挑戰「ㄈ」與「ㄏ」聲母區別之名句。",
        "usage": "「黑化肥發黑會揮發，灰化肥發灰會揮黑。」",
        "ruhi_weight": 995,
        "transitions": [["黑化肥", "發黑", 35.0], ["發黑", "會", 30.0], ["會", "揮發", 35.0]]
    },
    {
        "word": "灰化肥發灰會揮黑",
        "category": "saying",
        "origin": "經典唇齒音相聲繞口令《化肥會揮發》",
        "def": "極限挑戰聲韻轉化之繞口令對應句。",
        "usage": "「灰化肥發灰會揮黑，考驗說話吐字功力。」",
        "ruhi_weight": 995,
        "transitions": [["灰化肥", "發灰", 35.0], ["發灰", "會", 30.0], ["會", "揮黑", 35.0]]
    },
    {
        "word": "鰨蟆",
        "category": "saying",
        "origin": "《爾雅》魚名 / 民間相聲《喇嘛與啞巴》",
        "def": "比目魚古稱，俗稱鰨沙魚或比目魚，繞口令中喇嘛手裡所提之物。",
        "usage": "「南邊來了個喇嘛，手裡提著五斤鰨蟆。」",
        "ruhi_weight": 950,
        "transitions": [["五斤", "鰨蟆", 40.0]]
    },
    {
        "word": "粉紅牆上畫鳳凰",
        "category": "saying",
        "origin": "相聲繞口令《粉紅牆與鳳凰》",
        "def": "講究色彩鮮明與聲調起伏之經典繞口令。",
        "usage": "「粉紅牆上畫鳳凰，鳳凰畫在粉紅牆。」",
        "ruhi_weight": 995,
        "transitions": [["粉紅牆", "上", 35.0], ["上", "畫鳳凰", 35.0], ["鳳凰", "畫在", 35.0], ["畫在", "粉紅牆", 40.0]]
    },
    {
        "word": "和尚端湯上塔",
        "category": "saying",
        "origin": "相聲繞口令《和尚端湯》",
        "def": "挑戰「ㄊ」音連續吐字之經典相聲名段。",
        "usage": "「和尚端湯上塔，塔滑湯灑湯燙塔。」",
        "ruhi_weight": 995,
        "transitions": [["和尚", "端湯", 35.0], ["端湯", "上塔", 35.0], ["上塔", "塔滑", 35.0], ["塔滑", "湯灑", 35.0], ["湯灑", "湯燙塔", 40.0]]
    },
    {
        "word": "七巷一個漆匠",
        "category": "saying",
        "origin": "相聲繞口令《漆匠與錫匠》",
        "def": "區分舌面音「ㄑ」與「ㄒ」之經典行業繞口令名篇。",
        "usage": "「七巷一個漆匠，西巷一個錫匠，七巷漆匠用了西巷錫匠的錫杖。」",
        "ruhi_weight": 995,
        "transitions": [["七巷", "一個", 30.0], ["一個", "漆匠", 35.0], ["西巷", "一個", 30.0], ["一個", "錫匠", 35.0], ["漆匠", "用了", 35.0], ["錫匠", "的", 30.0], ["的", "錫杖", 35.0]]
    },

    # 4. 現代高頻活語言與日常短語
    {
        "word": "先這樣",
        "category": "modern",
        "origin": "現代通俗口語",
        "def": "簡短利落的告一段落結束詞，常用於通話結束、公事交辦告段落之際。",
        "usage": "「今天先討論到這裡，先這樣，拜拜！」",
        "ruhi_weight": 980,
        "transitions": [["先", "這樣", 30.0], ["先這樣", "吧", 25.0]]
    },
    {
        "word": "比倒讚",
        "category": "modern",
        "origin": "網路社群流行手勢語",
        "def": "大拇指向下表示否定、不認同、喝倒彩或鄙視的手勢動態。",
        "usage": "「這群瘋狂的年輕人很喜歡對人比倒讚。」",
        "ruhi_weight": 950,
        "transitions": [["比", "倒讚", 35.0]]
    },
    {
        "word": "騎車兜風",
        "category": "modern",
        "origin": "現代生活休閒熟語",
        "def": "騎乘機車或自行車漫無目的迎風奔馳，享受悠閒時光。",
        "usage": "「週末傍晚，他最喜歡騎車兜風放鬆心情。」",
        "ruhi_weight": 950,
        "transitions": [["騎車", "兜風", 35.0]]
    },
    {
        "word": "只顧著賣慘",
        "category": "modern",
        "origin": "現代社群流行熟語",
        "def": "只一味向外人哭訴自身境遇淒慘博取同情，而不思反省振作。",
        "usage": "「每到淡季大家就忘記旺季時賺錢的模樣，只顧著賣慘。」",
        "ruhi_weight": 950,
        "transitions": [["只顧著", "賣慘", 35.0]]
    },
    {
        "word": "如喜",
        "category": "modern",
        "origin": "RuhI 輸入法專有名詞",
        "def": "取「吉祥如意、滿心歡喜」之意，臺灣首款以純離線隱私為核心之許氏雙拼智慧型輸入法。",
        "usage": "「如喜輸入法如行雲流水，神準不選字。」",
        "ruhi_weight": 980,
        "transitions": [["如", "喜", 35.0]]
    }
]

def populate():
    print(f">>> 正在自動補齊注音與拼音，並寫入字戀共通擴充詞庫...")
    
    # 讀取既有詞庫
    existing = {}
    if os.path.exists(SUPPLEMENTARY_PATH):
        try:
            with open(SUPPLEMENTARY_PATH, "r", encoding="utf-8") as f:
                raw = json.load(f)
                for item in raw:
                    existing[item["word"]] = item
        except Exception:
            pass

    for entry in ENTRIES:
        w = entry["word"]
        # 自動透過 pypinyin 提取標準注音與漢語拼音
        bps = pypinyin.pinyin(w, style=pypinyin.BOPOMOFO)
        bp_str = " ".join([b[0] for b in bps])
        pys = pypinyin.pinyin(w, style=pypinyin.TONE)
        py_str = " ".join([p[0] for p in pys])

        # 針對特殊破音字校正
        if w == "得來全不費工夫" or w == "全不費工夫":
            bp_str = "ㄉㄜˊ ㄌㄞˊ ㄑㄩㄢˊ ㄅㄨˊ ㄈㄟˋ ㄍㄨㄥ ㄈㄨ"
        elif w == "鰨蟆":
            bp_str = "ㄊㄚˇ ˙ㄇㄚ"
        elif w == "和尚端湯上塔":
            bp_str = "ㄏㄜˊ ㄕㄤˋ ㄉㄨㄢ ㄊㄤ ㄕㄤˋ ㄊㄚˇ"
        elif w == "先這樣":
            bp_str = "ㄒㄧㄢ ㄓㄜˋ ㄧㄤˋ"

        entry["bopomofo"] = bp_str
        entry["pinyin"] = py_str
        entry["is_idiom"] = True

        existing[w] = entry

    # 輸出存檔
    result_list = list(existing.values())
    with open(SUPPLEMENTARY_PATH, "w", encoding="utf-8") as f:
        json.dump(result_list, f, ensure_ascii=False, indent=2)

    print(f">>> [OK] 成功更新 {SUPPLEMENTARY_PATH}，總擴充條目數: {len(result_list):,} 條！")

if __name__ == '__main__':
    populate()
