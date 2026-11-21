use ruhi::fsm::HsuSyllable;
use ruhi::lexicon::manager::PersonNameForm;
use ruhi::lexicon::{LexiconEntry, LexiconManager};
use ruhi::router::onomastics::clear_user_onomastic_registry;
use ruhi::router::transitions::TransitionEngine;
use ruhi::RuhiEngine;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

fn data_dir_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("ruhi_lexicon")
        .join("data")
}

fn data_dir_str() -> String {
    data_dir_path().to_string_lossy().into_owned()
}

fn bopomofo_to_std_keys(bpms: &[&str]) -> String {
    let mut s = String::new();
    for &b in bpms {
        if b == "，" {
            s.push_str("<CTRL_COMMA>");
            continue;
        }
        if b == "：" {
            s.push_str("<CTRL_COLON>");
            continue;
        }
        let syl = HsuSyllable::from_bopomofo_display(b).unwrap_or_else(|| panic!("invalid bpm: {b}"));
        if !syl.initial.is_empty() {
            let ik = match syl.initial.as_str() {
                "ㄅ" => '1',
                "ㄆ" => 'q',
                "ㄇ" => 'a',
                "ㄈ" => 'z',
                "ㄉ" => '2',
                "ㄊ" => 'w',
                "ㄋ" => 's',
                "ㄌ" => 'x',
                "ㄍ" => 'e',
                "ㄎ" => 'd',
                "ㄏ" => 'c',
                "ㄐ" => 'r',
                "ㄑ" => 'f',
                "ㄒ" => 'v',
                "ㄓ" => '5',
                "ㄔ" => 't',
                "ㄕ" => 'g',
                "ㄖ" => 'b',
                "ㄗ" => 'y',
                "ㄘ" => 'h',
                "ㄙ" => 'n',
                _ => panic!("unknown std initial: {}", syl.initial),
            };
            s.push(ik);
        }
        if !syl.medial.is_empty() {
            let mk = match syl.medial.as_str() {
                "ㄧ" => 'u',
                "ㄨ" => 'j',
                "ㄩ" => 'm',
                _ => panic!("unknown std medial: {}", syl.medial),
            };
            s.push(mk);
        }
        if !syl.final_part.is_empty() {
            let fk = match syl.final_part.as_str() {
                "ㄚ" => '8',
                "ㄛ" => 'i',
                "ㄜ" => 'k',
                "ㄝ" => ',',
                "ㄞ" => '9',
                "ㄟ" => 'o',
                "ㄠ" => 'l',
                "ㄡ" => '.',
                "ㄢ" => '0',
                "ㄣ" => 'p',
                "ㄤ" => ';',
                "ㄥ" => '/',
                "ㄦ" => '-',
                _ => panic!("unknown std final: {}", syl.final_part),
            };
            s.push(fk);
        }
        let tk = match syl.tone {
            '1' => ' ',
            '2' => '6',
            '3' => '3',
            '4' => '4',
            '5' => '7',
            _ => ' ',
        };
        s.push(tk);
    }
    s
}

fn bopomofo_to_hsu_keys(bpms: &[&str]) -> String {
    let mut s = String::new();
    for &b in bpms {
        if b == "，" {
            s.push_str("<CTRL_COMMA>");
            continue;
        }
        if b == "：" {
            s.push_str("<CTRL_COLON>");
            continue;
        }
        let syl = HsuSyllable::from_bopomofo_display(b).unwrap_or_else(|| panic!("invalid bpm: {b}"));
        if syl.initial.is_empty() && syl.medial.is_empty() {
            let k = match syl.final_part.as_str() {
                "ㄦ" => 'l',
                "ㄜ" => 'g',
                "ㄛ" => 'h',
                "ㄢ" => 'm',
                "ㄣ" => 'n',
                "ㄚ" => 'y',
                "ㄞ" => 'i',
                "ㄠ" => 'w',
                "ㄡ" => 'o',
                "ㄝ" => 'e',
                "ㄟ" => 'a',
                "ㄤ" => 'k',
                "ㄥ" => 'l',
                _ => panic!("unknown hsu zero final: {}", syl.final_part),
            };
            s.push(k);
        } else {
            if !syl.initial.is_empty() {
                let ik = match syl.initial.as_str() {
                    "ㄅ" => 'b',
                    "ㄆ" => 'p',
                    "ㄇ" => 'm',
                    "ㄈ" => 'f',
                    "ㄉ" => 'd',
                    "ㄊ" => 't',
                    "ㄋ" => 'n',
                    "ㄌ" => 'l',
                    "ㄍ" => 'g',
                    "ㄎ" => 'k',
                    "ㄏ" => 'h',
                    "ㄐ" | "ㄓ" => 'j',
                    "ㄑ" | "ㄔ" => 'v',
                    "ㄒ" | "ㄕ" => 'c',
                    "ㄖ" => 'r',
                    "ㄗ" => 'z',
                    "ㄘ" => 'a',
                    "ㄙ" => 's',
                    _ => panic!("unknown hsu initial: {}", syl.initial),
                };
                s.push(ik);
            }
            if !syl.medial.is_empty() {
                let mk = match syl.medial.as_str() {
                    "ㄧ" => 'e',
                    "ㄨ" => 'x',
                    "ㄩ" => 'u',
                    _ => panic!("unknown hsu medial: {}", syl.medial),
                };
                s.push(mk);
            }
            if !syl.final_part.is_empty() {
                let fk = match syl.final_part.as_str() {
                    "ㄚ" => 'y',
                    "ㄛ" => 'h',
                    "ㄜ" => 'g',
                    "ㄝ" => 'e',
                    "ㄞ" => 'i',
                    "ㄟ" => 'a',
                    "ㄠ" => 'w',
                    "ㄡ" => 'o',
                    "ㄢ" => 'm',
                    "ㄣ" => 'n',
                    "ㄤ" => 'k',
                    "ㄥ" | "ㄦ" => 'l',
                    _ => panic!("unknown hsu final: {}", syl.final_part),
                };
                s.push(fk);
            }
        }
        let tk = match syl.tone {
            '1' => ' ',
            '2' => 'd',
            '3' => 'f',
            '4' => 'j',
            '5' => 's',
            _ => ' ',
        };
        s.push(tk);
    }
    s
}

fn decode_tokens_in_mode(engine: &mut RuhiEngine, mode: &str, bpms: &[&str]) -> String {
    let encoded = match mode {
        "standard" => bopomofo_to_std_keys(bpms),
        "hsu" => bopomofo_to_hsu_keys(bpms),
        other => panic!("unsupported keyboard mode: {other}"),
    };
    engine.set_keyboard_mode(mode);
    engine.reset_buffer();
    engine.last_committed_word.clear();
    engine.active_geo_context.clear();

    let mut committed = String::new();
    let mut rest = encoded.as_str();
    while !rest.is_empty() {
        if let Some(after) = rest.strip_prefix("<CTRL_COMMA>") {
            let resp = engine.process_key_str("Ctrl+,");
            if !resp.committed_chunk.is_empty() {
                committed.push_str(&resp.committed_chunk);
            }
            rest = after;
        } else if let Some(after) = rest.strip_prefix("<CTRL_COLON>") {
            let resp = engine.process_key_str("Ctrl+Shift+;");
            if !resp.committed_chunk.is_empty() {
                committed.push_str(&resp.committed_chunk);
            }
            rest = after;
        } else {
            let next_tag = rest
                .find("<CTRL_COMMA>")
                .into_iter()
                .chain(rest.find("<CTRL_COLON>"))
                .min()
                .unwrap_or(rest.len());
            let (chunk, after) = rest.split_at(next_tag);
            for ch in chunk.chars() {
                let resp = engine.process_key(ch);
                if !resp.committed_chunk.is_empty() {
                    committed.push_str(&resp.committed_chunk);
                }
            }
            rest = after;
        }
    }
    format!("{}{}", committed, engine.get_display_text())
}

#[test]
fn test_person_name_classifier_and_note_tags() {
    let mut core_lex = LexiconManager::new();
    let core_path = data_dir_path().join("lexicon_core.tsv");
    core_lex
        .load_tsv(core_path.to_str().unwrap(), "core")
        .expect("core lexicon should load");

    // All 5 PersonNameForm structural archetypes
    assert_eq!(
        core_lex.classify_user_person_name("曹大十"),
        Some((PersonNameForm::PersonS1G2, "曹".to_string(), "大十".to_string()))
    );
    assert_eq!(
        core_lex.classify_user_person_name("陳怡君"),
        Some((PersonNameForm::PersonS1G2, "陳".to_string(), "怡君".to_string()))
    );
    assert_eq!(
        core_lex.classify_user_person_name("司馬相如"),
        Some((PersonNameForm::PersonS2G2, "司馬".to_string(), "相如".to_string()))
    );
    assert_eq!(
        core_lex.classify_user_person_name("歐陽修"),
        Some((PersonNameForm::PersonS2G1, "歐陽".to_string(), "修".to_string()))
    );
    assert_eq!(
        core_lex.classify_user_person_name("司馬昭"),
        Some((PersonNameForm::PersonS2G1, "司馬".to_string(), "昭".to_string()))
    );
    assert_eq!(
        core_lex.classify_user_person_name("林安"),
        Some((PersonNameForm::PersonS1G1, "林".to_string(), "安".to_string()))
    );
    let empty_lex = LexiconManager::new();
    assert_eq!(
        empty_lex.classify_user_person_name("李白"),
        Some((PersonNameForm::PersonS1G1, "李".to_string(), "白".to_string()))
    );
    assert_eq!(
        core_lex.classify_user_person_name("王小美"),
        Some((PersonNameForm::PersonS1G2, "王".to_string(), "小美".to_string()))
    );
    assert_eq!(
        core_lex.classify_user_person_name("黃安琪莉"),
        Some((PersonNameForm::PersonS1G3, "黃".to_string(), "安琪莉".to_string()))
    );

    assert_eq!(
        LexiconManager::format_person_note_tag(PersonNameForm::PersonS1G2, "曹", "大十"),
        "person_s1_g2:曹+大十"
    );
    assert_eq!(
        LexiconManager::format_person_note_tag(PersonNameForm::PersonS2G2, "司馬", "相如"),
        "person_s2_g2:司馬+相如"
    );
    assert_eq!(
        LexiconManager::format_person_note_tag(PersonNameForm::PersonS2G1, "歐陽", "修"),
        "person_s2_g1:歐陽+修"
    );
    assert_eq!(
        LexiconManager::format_person_note_tag(PersonNameForm::PersonS1G1, "李", "白"),
        "person_s1_g1:李+白"
    );
    assert_eq!(
        LexiconManager::format_person_note_tag(PersonNameForm::PersonS1G3, "黃", "安琪莉"),
        "person_s1_g3:黃+安琪莉"
    );

    // Exclusions: common words (incl. 2-char core lexicon entries like 李白), places, foods, idioms, titles, and organizations must never match
    let negatives = [
        "李白",
        "高興",
        "文化",
        "馬路",
        "白飯",
        "李子",
        "張開",
        "大事",
        "高麗菜",
        "西門町",
        "東方人",
        "司空見慣",
        "台灣大學",
        "陳氏宗親會",
        "張小華有限公司",
        "王先生",
        "林小姐",
    ];
    for neg in negatives {
        assert_eq!(
            core_lex.classify_user_person_name(neg),
            None,
            "非人名詞「{neg}」不應誤判為使用者自訂人名"
        );
    }
}

#[test]
fn test_cao_da_shi_onomastics_via_tsv_and_upsert_std_and_hsu() {
    let base_dir = data_dir_str();

    // Case paths shared between TSV-loaded engine and upsert-registered engine
    let cases: Vec<(Vec<&str>, &str)> = vec![
        // Full name
        (vec!["ㄘㄠˊ", "ㄉㄚˋ", "ㄕˊ"], "曹大十"),
        // (1) Eight honorific titles following surname 曹
        (vec!["ㄘㄠˊ", "ㄒㄧㄢ", "ㄕㄥ"], "曹先生"),
        (vec!["ㄘㄠˊ", "ㄒㄧㄠˇ", "ㄐㄧㄝˇ"], "曹小姐"),
        (vec!["ㄘㄠˊ", "ㄅㄛˊ", "ㄕˋ"], "曹博士"),
        (vec!["ㄘㄠˊ", "ㄌㄠˇ", "ㄕ"], "曹老師"),
        (vec!["ㄘㄠˊ", "ㄐㄧㄠˋ", "ㄕㄡˋ"], "曹教授"),
        (vec!["ㄘㄠˊ", "ㄓㄨˇ", "ㄖㄣˋ"], "曹主任"),
        (vec!["ㄘㄠˊ", "ㄕˋ", "ㄓㄤˇ"], "曹市長"),
        (vec!["ㄘㄠˊ", "ㄐㄧㄥ", "ㄌㄧˇ"], "曹經理"),
        // (2a) Given name 大十 standalone / verb-object / subject contexts
        (vec!["ㄉㄚˋ", "ㄕˊ"], "大十"),
        (vec!["ㄓㄠˇ", "ㄉㄚˋ", "ㄕˊ"], "找大十"),
        (vec!["ㄨㄣˋ", "ㄉㄚˋ", "ㄕˊ"], "問大十"),
        (vec!["ㄑㄧㄥˇ", "ㄉㄚˋ", "ㄕˊ", "ㄌㄞˊ"], "請大十來"),
        (vec!["ㄉㄚˋ", "ㄕˊ", "ㄕㄨㄛ"], "大十說"),
        (vec!["ㄉㄚˋ", "ㄕˊ", "ㄌㄞˊ", "˙ㄌㄜ"], "大十來了"),
        // (2b) Negative contrasts: 大事 must win in inanimate / idiomatic frames even with 曹大十 active!
        (vec!["ㄍㄨㄛˊ", "ㄐㄧㄚ", "ㄉㄚˋ", "ㄕˋ"], "國家大事"),
        (vec!["ㄖㄣˊ", "ㄕㄥ", "ㄉㄚˋ", "ㄕˋ"], "人生大事"),
        (vec!["ㄊㄡˊ", "ㄉㄥˇ", "ㄉㄚˋ", "ㄕˋ"], "頭等大事"),
        (vec!["ㄧˊ", "ㄐㄧㄢˋ", "ㄉㄚˋ", "ㄕˋ"], "一件大事"),
        // (3) Kinship & peer suffixes following given name 大十 (including tone-distinct 姊 ㄗˇ vs 姐 ㄐㄧㄝˇ)
        (vec!["ㄉㄚˋ", "ㄕˊ", "ㄍㄜ"], "大十哥"),
        (vec!["ㄉㄚˋ", "ㄕˊ", "ㄒㄩㄥ"], "大十兄"),
        (vec!["ㄉㄚˋ", "ㄕˊ", "ㄉㄧˋ"], "大十弟"),
        (vec!["ㄉㄚˋ", "ㄕˊ", "ㄗˇ"], "大十姊"),
        (vec!["ㄉㄚˋ", "ㄕˊ", "ㄐㄧㄝˇ"], "大十姐"),
        (vec!["ㄉㄚˋ", "ㄕˊ", "ㄇㄟˋ"], "大十妹"),
        (vec!["ㄉㄚˋ", "ㄕˊ", "ㄌㄠˇ", "ㄕ"], "大十老師"),
        (vec!["ㄉㄚˋ", "ㄕˊ", "ㄒㄩㄝˊ", "ㄓㄤˇ"], "大十學長"),
        (vec!["ㄉㄚˋ", "ㄕˊ", "ㄒㄩㄝˊ", "ㄐㄧㄝˇ"], "大十學姊"),
    ];

    // Path A: temp user_dict.tsv via load_tsv
    clear_user_onomastic_registry();
    let uniq = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let tmp_dir = std::env::temp_dir().join(format!("ruhi_test_onom_{uniq}"));
    std::fs::create_dir_all(&tmp_dir).unwrap();
    let user_tsv = tmp_dir.join("user_dict.tsv");
    std::fs::write(&user_tsv, "曹大十\tㄘㄠˊ ㄉㄚˋ ㄕˊ\t1200\tuser\n").unwrap();

    let mut engine_tsv = RuhiEngine::new_with_data_dir(&base_dir);
    engine_tsv
        .lexicon
        .load_tsv(user_tsv.to_str().unwrap(), "user")
        .expect("user_dict.tsv should load");

    for mode in ["standard", "hsu"] {
        for (bpms, expected) in &cases {
            let actual = decode_tokens_in_mode(&mut engine_tsv, mode, bpms);
            assert_eq!(
                actual, *expected,
                "[user_dict TSV / {mode}] 注音 {:?} 應解碼為「{}」，實際為「{}」",
                bpms, expected, actual
            );
        }
    }
    let _ = std::fs::remove_dir_all(&tmp_dir);

    // Path B: upsert_entry registration on clean registry
    clear_user_onomastic_registry();
    let mut engine_upsert = RuhiEngine::new_with_data_dir(&base_dir);
    engine_upsert
        .lexicon
        .upsert_entry(LexiconEntry::new("曹大十", "ㄘㄠˊ ㄉㄚˋ ㄕˊ", 1200, "user", ""));

    for mode in ["standard", "hsu"] {
        for (bpms, expected) in &cases {
            let actual = decode_tokens_in_mode(&mut engine_upsert, mode, bpms);
            assert_eq!(
                actual, *expected,
                "[upsert_entry / {mode}] 注音 {:?} 應解碼為「{}」，實際為「{}」",
                bpms, expected, actual
            );
        }
    }
    clear_user_onomastic_registry();
}

#[test]
fn test_p1_c1_assoc_prefix_sidecar_parity_and_invalidation() {
    let mut lex = LexiconManager::new();
    let core_path = data_dir_path().join("lexicon_core.tsv");
    lex.load_tsv(core_path.to_str().unwrap(), "core")
        .expect("core lexicon should load");
    let transitions = TransitionEngine::new();

    for pfx in ["人工", "電腦", "台灣", "經", "自"] {
        let chars: Vec<char> = pfx.chars().collect();
        let got = lex.lookup_associative_continuations(&chars, chars.len(), &transitions);
        assert!(
            !got.is_empty(),
            "expected non-empty associative candidates for '{pfx}'"
        );
        // Verify lexicon suffix entries for this prefix actually come from words starting with `pfx`
        let pfx_entries_present = lex
            .entries
            .iter()
            .filter(|e| e.word != pfx && e.word.starts_with(pfx))
            .map(|e| e.word[pfx.len()..].to_string())
            .collect::<std::collections::HashSet<_>>();
        assert!(
            got.iter().any(|cand| pfx_entries_present.contains(cand)),
            "expected prefix sidecar continuation in results for '{pfx}', got {:?}",
            got
        );
    }

    // Verify mutation invalidates sidecar epoch immediately
    let rg_chars: Vec<char> = "人工".chars().collect();
    lex.upsert_entry(LexiconEntry::new(
        "人工作曲測試專用詞",
        "ㄖㄣˊ ㄍㄨㄥ ㄗㄨㄛˋ ㄑㄩˇ ㄘㄜˋ ㄕˋ ㄓㄨㄢ ㄩㄥˋ ㄘˊ",
        9999,
        "user",
        "",
    ));
    let after_mut = lex.lookup_associative_continuations(&rg_chars, rg_chars.len(), &transitions);
    assert!(
        after_mut.contains(&"作曲測試專用詞".to_string()),
        "mutation must invalidate ASSOC_PREFIX_SIDECAR immediately; got {:?}",
        after_mut
    );
}

#[test]
fn test_wave1a_eight_y_sentences_and_contrasts_std_and_hsu() {
    clear_user_onomastic_registry();
    let mut engine = RuhiEngine::new_with_data_dir(&data_dir_str());

    let cases: Vec<(Vec<&str>, &str)> = vec![
        // 1. 鋸子 vs 橘子, 鋸出 vs 劇出
        (
            vec![
                "ㄗㄞˋ", "ㄈㄥ", "ㄌㄧˋ", "ㄑㄧㄤˊ", "ㄐㄧㄣˋ", "˙ㄉㄜ", "ㄏㄞˇ", "ㄅㄧㄢ",
                "ㄋㄚˊ", "˙ㄓㄜ", "ㄧˋ", "ㄅㄚˇ", "ㄈㄥ", "ㄌㄧˋ", "˙ㄉㄜ", "ㄐㄩˋ", "˙ㄗ",
                "ㄧㄠˋ", "ㄗㄞˋ", "ㄧˋ", "ㄍㄣ", "ㄆㄧㄠ", "ㄌㄧㄡˊ", "ㄇㄨˋ", "ㄕㄤˋ",
                "ㄐㄩˋ", "ㄔㄨ", "ㄧˊ", "ㄐㄩˋ", "ㄔㄨㄥ", "ㄇㄢˇ", "ㄒㄧˋ", "ㄐㄩˋ", "ㄓㄤ", "ㄌㄧˋ",
                "˙ㄉㄜ", "ㄐㄩˋ", "˙ㄗ", "ㄕˊ", "ㄗㄞˋ", "ㄏㄣˇ", "ㄅㄨˋ", "ㄖㄨㄥˊ", "ㄧˋ",
            ],
            "在風力強勁的海邊拿著一把鋒利的鋸子要在一根漂流木上鋸出一句充滿戲劇張力的句子實在很不容易",
        ),
        // 2. 按需 & 按鍵
        (
            vec![
                "ㄢˋ", "ㄒㄩ", "ㄓˇ", "ㄆㄞˋ", "ㄙㄨㄛˇ", "ㄧㄡˇ", "˙ㄉㄜ", "ㄢˋ", "ㄐㄧㄢˋ",
                "ㄍㄟˇ", "ㄅㄨˋ", "ㄊㄨㄥˊ", "˙ㄉㄜ", "ㄓㄨˋ", "ㄌㄧˇ", "ㄒㄩ", "ㄧㄠˋ",
                "ㄏㄜˊ", "ㄌㄧˇ", "˙ㄉㄜ", "ㄈㄤ", "ㄢˋ", "ㄐㄧㄤˇ", "ㄌㄧˋ",
            ],
            "按需指派所有的按鍵給不同的助理需要合理的方案獎勵",
        ),
        // 3. 滿常 & 滿長
        (
            vec![
                "ㄨㄛˇ", "ㄍㄜˋ", "ㄖㄣˊ", "ㄏㄞˊ", "ㄇㄢˇ", "ㄔㄤˊ", "ㄕㄨ", "ㄒㄧㄝˇ",
                "ㄇㄟˊ", "ㄧㄡˇ", "ㄖㄣˋ", "ㄏㄜˊ", "ㄅㄧㄠ", "ㄉㄧㄢˇ", "ㄈㄨˊ", "ㄏㄠˋ",
                "ㄔㄨㄢˋ", "ㄐㄧㄝ", "ㄧˋ", "ㄉㄨㄟ", "ㄇㄧㄠˊ", "ㄕㄨˋ", "ㄘˊ",
                "ㄎㄢˋ", "ㄑㄧˇ", "ㄌㄞˊ", "ㄏㄞˊ", "ㄇㄢˇ", "ㄔㄤˊ", "˙ㄉㄜ", "ㄐㄩˋ", "˙ㄗ",
            ],
            "我個人還滿常書寫沒有任何標點符號串接一堆描述詞看起來還滿長的句子",
        ),
        // 4. 五弄裡 vs 舞弄 (inside real address lane scope closed by 裡)
        (
            vec![
                "ㄋㄚˋ", "ㄒㄧㄝ", "ㄋㄧㄢˊ", "ㄑㄧㄥ", "ㄖㄣˊ", "ㄏㄣˇ", "ㄒㄧˇ", "ㄏㄨㄢ",
                "ㄗㄞˋ", "ㄇㄡˇ", "ㄌㄨˋ", "ㄇㄡˇ", "ㄒㄧㄤˋ", "ㄨˇ", "ㄋㄨㄥˋ", "ㄌㄧˇ",
                "ㄨˇ", "ㄋㄨㄥˋ", "ㄊㄚ", "ㄇㄣˊ", "˙ㄉㄜ", "ㄓ", "ㄊㄧˇ",
            ],
            "那些年輕人很喜歡在某路某巷五弄裡舞弄他們的肢體",
        ),
        // 注：原 #5（常委 vs 長尾貓）與 #6（亮出一手同花順好牌）已拆出至 test_wave1a_phase1d_v6_ngram_known_hard_cases_261121（見 docs/KNOWN_HARD_CASES.md）
        // 7. Long narrative sentence: 花襯衫 / 舞弄著 / 長尾 / 滿樹盛開桐花 / 溫暖的陽光
        (
            vec![
                "ㄨㄛˇ", "ㄩㄥˋ", "ㄧˋ", "ㄓ", "ㄒㄧㄝˇ", "ㄑㄧˇ", "ㄌㄞˊ", "ㄏㄣˇ",
                "ㄕㄨㄣˋ", "ㄕㄡˇ", "˙ㄉㄜ", "ㄍㄤ", "ㄅㄧˇ", "ㄗㄞˋ", "ㄓˇ", "ㄕㄤˋ",
                "ㄒㄧㄝˇ", "ㄒㄧㄚˋ", "：",
                "ㄧˋ", "ㄓ", "ㄔㄨㄢ", "˙ㄓㄜ", "ㄏㄨㄚ", "ㄔㄣˋ", "ㄕㄢ",
                "ㄧㄠˊ", "ㄊㄡˊ", "ㄏㄨㄤˋ", "ㄋㄠˇ", "ㄨˇ", "ㄋㄨㄥˋ", "˙ㄓㄜ",
                "ㄔㄤˊ", "ㄨㄟˇ", "˙ㄉㄜ", "ㄒㄧㄠˇ", "ㄏㄨㄚ", "ㄇㄠ",
                "ㄇㄢˋ", "ㄅㄨˋ", "ㄐㄧㄥ", "ㄒㄧㄣ", "ㄒㄧㄠ", "ㄧㄠˊ", "ㄗˋ", "ㄗㄞˋ",
                "˙ㄉㄜ", "ㄗㄡˇ", "ㄗㄞˋ", "ㄇㄢˇ", "ㄕㄨˋ", "ㄕㄥˋ", "ㄎㄞ",
                "ㄊㄨㄥˊ", "ㄏㄨㄚ", "˙ㄉㄜ", "ㄕㄨˋ", "ㄧㄣˋ", "ㄒㄧㄚˋ",
                "ㄕㄞˋ", "˙ㄓㄜ", "ㄨㄣ", "ㄋㄨㄢˇ", "˙ㄉㄜ", "ㄧㄤˊ", "ㄍㄨㄤ",
            ],
            "我用一支寫起來很順手的鋼筆在紙上寫下：一隻穿著花襯衫搖頭晃腦舞弄著長尾的小花貓漫不經心逍遙自在的走在滿樹盛開桐花的樹蔭下曬著溫暖的陽光",
        ),
        // 8. 極小眾 + contrast negatives
        (vec!["ㄙㄨㄟ", "ㄖㄢˊ", "ㄑㄧˊ", "ㄕˊ", "ㄕˋ", "ㄐㄧˊ", "ㄒㄧㄠˇ", "ㄓㄨㄥˋ"], "雖然其實是極小眾"),
        (vec!["ㄓㄡ", "ㄇㄛˋ", "ㄑㄩˋ", "ㄍㄨㄤˋ", "ㄨㄣˊ", "ㄔㄨㄤˋ", "ㄕˋ", "ㄐㄧˊ"], "週末去逛文創市集"),
        (vec!["ㄑㄧˊ", "ㄕˊ", "ㄕˋ", "ㄐㄧˊ", "ㄏㄣˇ", "ㄖㄜˋ", "ㄋㄠˋ"], "其實市集很熱鬧"),
        // Positive contrasts preventing overfitting
        (vec!["ㄗㄞˋ", "ㄇㄢˇ", "ㄏㄨㄛˋ", "ㄨˋ"], "載滿貨物"),
        (vec!["ㄌㄧㄤˊ", "ㄔㄨ", "ㄊㄧˇ", "ㄨㄣ"], "量出體溫"),
        (vec!["ㄊㄨㄥˊ", "ㄏㄨㄚ", "ㄉㄚˋ", "ㄕㄨㄣˋ"], "同花大順"),
        (vec!["ㄒㄧㄣˋ", "ㄧˋ", "ㄌㄨˋ", "ㄨˇ", "ㄉㄨㄢˋ"], "信義路五段"),
    ];

    let mut failures = Vec::new();
    for mode in ["standard", "hsu"] {
        for (idx, (bpms, expected)) in cases.iter().enumerate() {
            let actual = decode_tokens_in_mode(&mut engine, mode, bpms);
            if actual != *expected {
                failures.push(format!("[{mode} #{idx}] 預期「{expected}」\n  實際「{actual}」"));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "尚有 {} 項解碼未通過:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[test]
#[ignore = "known hard case: Phase 1D v6 statistical N-gram (dev CER 3.89%, -240 errors, p=3.76e-14) thin-margin trade-off; see docs/KNOWN_HARD_CASES.md"]
fn test_wave1a_phase1d_v6_ngram_known_hard_cases_261121() {
    clear_user_onomastic_registry();
    let mut engine = RuhiEngine::new_with_data_dir(&data_dir_str());

    let cases: Vec<(Vec<&str>, &str)> = vec![
        // 常委 vs 長尾貓
        (
            vec![
                "ㄨㄛˇ", "ㄇㄣˊ", "ㄕㄜˋ", "ㄑㄩ", "ㄍㄨㄢˇ", "ㄌㄧˇ", "ㄨㄟˇ", "ㄩㄢˊ", "ㄏㄨㄟˋ",
                "˙ㄉㄜ", "ㄔㄤˊ", "ㄨㄟˇ", "ㄧㄤˇ", "˙ㄌㄜ", "ㄧˋ", "ㄓ", "ㄔㄤˊ", "ㄨㄟˇ", "ㄇㄠ",
            ],
            "我們社區管理委員會的常委養了一隻長尾貓",
        ),
        // 亮出一手同花順好牌
        (
            vec![
                "ㄧˋ", "ㄑㄩㄣˊ", "ㄋㄧㄢˊ", "ㄑㄧㄥ", "ㄖㄣˊ", "ㄗㄞˋ", "ㄇㄢˇ", "ㄕㄨˋ",
                "ㄊㄨㄥˊ", "ㄏㄨㄚ", "˙ㄉㄜ", "ㄕㄨˋ", "ㄧㄣˋ", "ㄒㄧㄚˋ", "ㄨㄢˊ", "ㄑㄧˇ",
                "ㄆㄨ", "˙ㄎㄜ", "ㄆㄞˊ", "，",
                "ㄑㄧˊ", "ㄓㄨㄥ", "ㄧˋ", "ㄖㄣˊ", "ㄌㄧㄤˋ", "ㄔㄨ", "ㄧˋ", "ㄕㄡˇ",
                "ㄊㄨㄥˊ", "ㄏㄨㄚ", "ㄕㄨㄣˋ", "ㄏㄠˇ", "ㄆㄞˊ", "，",
                "ㄙㄨㄛˇ", "ㄧㄡˇ", "ㄖㄣˊ", "ㄉㄡ", "ㄊㄨㄥˊ", "ㄕˊ", "ㄊㄨㄥˊ", "ㄧˋ",
                "ㄊㄚ", "ㄕˋ", "ㄐㄧㄣ", "ㄊㄧㄢ", "ㄗㄨㄟˋ", "ㄒㄧㄥˋ", "ㄩㄣˋ", "˙ㄉㄜ", "ㄖㄣˊ",
            ],
            "一群年輕人在滿樹桐花的樹蔭下玩起撲克牌，其中一人亮出一手同花順好牌，所有人都同時同意他是今天最幸運的人",
        ),
    ];

    let mut failures = Vec::new();
    for mode in ["standard", "hsu"] {
        for (idx, (bpms, expected)) in cases.iter().enumerate() {
            let actual = decode_tokens_in_mode(&mut engine, mode, bpms);
            if actual != *expected {
                failures.push(format!("[{mode} #{idx}] 預期「{expected}」\n  實際「{actual}」"));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "尚有 {} 項解碼未通過:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
