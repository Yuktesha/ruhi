# 📜 第三方授權與開源資產聲明 (Third-Party Licenses & Acknowledgements)

如喜（RuhI）通用智慧型輸入法秉持開放、尊重開源社群與法務合規的精神，在專案中整合、改進並借鑒了以下優秀開源專案與公開資料集之成果，謹此致上最誠摯的感謝。

---

## 1. 小麥注音 (McBopomofo)

* **使用範圍**：`ruhi_lexicon/data/lexicon_mcbopomofo.tsv` 注音詞頻統計資料與注音映射對照。
* **原始專案**：[https://github.com/openvanilla/McBopomofo](https://github.com/openvanilla/McBopomofo)
* **授權協議**：MIT License
* **著作權聲明**：
  ```text
  MIT License

  Copyright (c) 2011-2026 Meng-Cheng Hsieh et al.
  All rights reserved.

  Permission is hereby granted, free of charge, to any person obtaining a copy
  of this software and associated documentation files (the "Software"), to deal
  in the Software without restriction, including without limitation the rights
  to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
  copies of the Software, and to permit persons to whom the Software is
  furnished to do so, subject to the following conditions:

  The above copyright notice and this permission notice shall be included in all
  copies or substantial portions of the Software.

  THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
  IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
  FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
  AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
  LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
  OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
  SOFTWARE.
  ```

---

## 2. 中華民國教育部《國語辭典》公開資料集與 g0v 零時政府萌典 (MoeDict)

* **使用範圍**：`ruhi_lexicon/data/lexicon_moe.tsv` 標準國語注音、詞條與正體字音讀資料。
* **資料來源**：中華民國教育部《重編國語辭典修訂本》、教育部《國語辭典簡編本》開放資料，以及 g0v 零時政府萌典 (MoeDict) 結構化整理成果。
* **授權依據**：
  * **政府資料開放授權條款 (Open Government Data License - OGDL 1.0)**
  * **創用 CC 姓名標示-禁止改作 3.0 台灣授權條款 (CC BY-ND 3.0 TW)**（針對教育部歷史釋出版本之相容標示）
  * **CC0 1.0 公眾領域貢獻宣告**（針對 g0v 萌典專案之結構化轉檔程式與標記工具）
* **法定標示聲明**：
  > 本專案部分中文詞彙、字詞讀音與釋義資料取自中華民國教育部公開之語文辭典資料集（《重編國語辭典修訂本》及《國語辭典簡編本》），依「政府資料開放授權條款 (OGDL 1.0)」第 6 條及「創用 CC 姓名標示-禁止改作 3.0 台灣授權條款」規定顯著標明資料來源。資料內容版權歸屬於中華民國教育部。

---

## 3. Unicode Unihan Database (`Unihan.zip`)

* **使用範圍**：`scripts/build_longkan_tsv.py` 與 `scripts/build_four_corner_tsv.py` 建置期間讀取之 `Unihan_IRGSources.txt`（`kRSUnicode`, `kTotalStrokes`）、`Unihan_Readings.txt`（`kMandarin`, `kHanyuPinyin`, `kTGHZ2013`）、`Unihan_DictionaryLikeData.txt`（`kFourCornerCode`, `kPhonetic`）及 `Unihan_Variants.txt`（正異體對應）。
* **資料來源**：Unicode Consortium — Unicode Character Database (UCD) / Unihan Database ([https://www.unicode.org/Public/UCD/latest/ucd/Unihan.zip](https://www.unicode.org/Public/UCD/latest/ucd/Unihan.zip))
* **授權協議**：**Unicode License v3 (UNICODE, INC. LICENSE AGREEMENT - DATA FILES AND SOFTWARE)**（與 Apache License 2.0 完全相容之寬鬆開源授權）
* **著作權與授權聲明**：
  ```text
  UNICODE LICENSE V3
  COPYRIGHT AND PERMISSION NOTICE

  Copyright © 1991-2026 Unicode, Inc. All rights reserved.
  Distributed under the Terms of Use in https://www.unicode.org/copyright.html.

  Permission is hereby granted, free of charge, to any person obtaining a
  copy of the Unicode data files and any associated documentation (the "Data
  Files") or Unicode software and any associated documentation (the
  "Software") to deal in the Data Files or Software without restriction,
  including without limitation the rights to use, copy, modify, merge,
  publish, distribute, and/or sell copies of the Data Files or Software, and
  to permit persons to whom the Data Files or Software are furnished to do
  so, provided that either
  (a) this copyright and permission notice appear with all copies of the Data
      Files or Software, or
  (b) this copyright and permission notice appear in associated
      Documentation.
  ```

---

## 4. 倉頡五代字根碼表 (Cangjie Input Method Table)

* **使用範圍**：`ruhi_lexicon/data/cangjie.tsv` 倉頡輸入法五代字根與漢字編碼對照表。
* **說明**：朱邦復先生將倉頡輸入法公開發表、免收任何專利與版權費用，以促進中文資訊化之普及。本專案編碼表取自自由開放之公共領域碼表，並經現代化整理相容於如喜零切換智慧模式。

---

## 5. 行列 30 輸入法標準開源碼表與規格 (Array 30 Input Method Specification & Tables)

* **使用範圍**：`ruhi_lexicon/data/array30.tsv`（涵蓋完整字碼、一二級簡碼、特別碼與 `W` / `HG` 符號表）及「許氏注音 ➔ 行列 30 即時反查教練」字根座標映射。
* **發明人與授權來源**：行列科技 **廖明德先生**（行列輸入法發明人）公開無償授權釋出之《行列 30 輸入法 Porting 規格書》與標準對照碼表；並致謝 **葉光哲先生（老刀 / gontera）** 持續維護與校訂之開源專案 [https://github.com/gontera/array30](https://github.com/gontera/array30)（v2026 版）。
* **授權說明**：廖明德先生自發明行列輸入法 30 鍵版初始即實施無償開放授權路線，公開提供各作業系統與第三方輸入法軟體開發者自由植入與使用。

---

## 6. 歷史外部資料查證與移除紀錄：`ids.txt` (CJKVI / CHISE IDS)

* **查證對象**：早期工作區 `_archive/_external/ids.txt` 曾存放衍生自 CHISE Project（文字情報基盤整備事業）與 `cjkvi-ids`（Kawabata Taichi 維護）之表意文字描述序列（Ideographic Description Sequences, IDS）檔案，該檔案標頭採用 **GNU General Public License v2 (GPLv2)**。
* **合規處置紀錄（2026-10-29）**：
  1. 為確保如喜（RuhI）全專案 **100% 維持純淨之 Apache License 2.0 授權相容性**（避免任何 GPLv2 衍生爭議），本專案已將 `_archive/_external/ids.txt` 徹底刪除，並自 `scripts/build_longkan_tsv.py` 中完全移除所有對 `ids.txt` 之讀取與依賴。
  2. 現有龍龕形音碼表（`ruhi_lexicon/data/longkan.tsv` 與 `longkan.bin`）100% 僅由 **Unicode Unihan Database (`Unihan.zip`, Unicode License v3)**（含 `kRSUnicode` 部首餘筆、`kTotalStrokes` 筆畫數結構校驗、`kPhonetic` 聲符、`kMandarin` / `kHanyuPinyin` 讀音）結合本專案原創編寫之部件與會意字結構表（`MULTI_COMP_OVERRIDE`, `Apache-2.0`）獨立建置而成，不含任何 GPLv2 資料成分。

---

## 7. Rust 開源生態依賴組件 (Rust Ecosystem Crates)

本專案使用 Cargo 所建置之第三方 Rust 核心函式庫均依其原本之 MIT / Apache-2.0 雙授權條款遵循：

| Crate 名稱 | 授權條款 | 專案用途 |
| :--- | :--- | :--- |
| `windows` / `windows-core` / `windows-sys` | MIT OR Apache-2.0 | Microsoft 官方 Windows 11 API、COM 介面與 TSF 綁定 |
| `serde` / `serde_json` | MIT OR Apache-2.0 | 零拷貝資料序列化與 JSON 配置檔解析 |
| `bincode` | MIT | 高效二進位資料編解碼 |
| `memchr` | Unlicense OR MIT | 極速字串與位元組序列搜尋 |
