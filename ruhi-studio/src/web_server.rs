//! # `web_server` — 如喜語意調校工作室原生 HTTP 伺服器 (`127.0.0.1:8765`)
//!
//! 100% 相容原 `scripts/semantic_tuner.py` 之 `--web` 模式與 `scripts/web/index.html` 前端介面，
//! 採用純 Rust 標準庫 `std::net::TcpListener` 實作多執行緒無阻塞 REST API 服務。

use serde_json::{json, Value};
use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::ai_consultant::AISemanticConsultant;
use crate::tuner::SemanticTuner;

pub struct StudioServerState {
    pub tuner: SemanticTuner,
    pub consultant: AISemanticConsultant,
    pub preloaded_file: String,
}

/// 啟動本地 Web 語意調校工作室伺服器 (`127.0.0.1:8765`)
pub fn start_web_studio(
    tuner: SemanticTuner,
    mut port: u16,
    preloaded_file: String,
    open_browser: bool,
) -> Result<(), String> {
    let root = tuner.root.clone();
    let consultant = AISemanticConsultant::new(Some(&root));
    let state = Arc::new(Mutex::new(StudioServerState {
        tuner,
        consultant,
        preloaded_file,
    }));

    let listener = match TcpListener::bind(("127.0.0.1", port)) {
        Ok(l) => l,
        Err(_) => {
            port += 1;
            TcpListener::bind(("127.0.0.1", port))
                .map_err(|e| format!("無法綁定本地連接埠 {port}: {e}"))?
        }
    };

    let server_url = format!("http://127.0.0.1:{port}");
    println!("========================================================================");
    println!("  如喜輸入法 (RuhI) · 語意調校工作室 (Rust 原生 Web 伺服器)");
    println!("  本地伺服器位址：{server_url}");
    println!("========================================================================");

    if open_browser {
        let _ = Command::new("cmd.exe")
            .args(["/c", "start", "", &server_url])
            .spawn();
    }

    for stream in listener.incoming().flatten() {
        let st = Arc::clone(&state);
        std::thread::spawn(move || {
            let _ = handle_http_connection(stream, st);
        });
    }

    Ok(())
}

fn handle_http_connection(
    mut stream: TcpStream,
    state: Arc<Mutex<StudioServerState>>,
) -> Result<(), std::io::Error> {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(10)));

    let mut buffer = Vec::with_capacity(8192);
    let mut temp = [0u8; 4096];
    let mut header_end = None;
    let mut content_length = 0usize;

    loop {
        let n = match stream.read(&mut temp) {
            Ok(0) => break,
            Ok(n) => n,
            Err(_) => break,
        };
        buffer.extend_from_slice(&temp[..n]);
        if header_end.is_none() {
            if let Some(pos) = find_subsequence(&buffer, b"\r\n\r\n") {
                header_end = Some(pos + 4);
                let header_str = String::from_utf8_lossy(&buffer[..pos]);
                for line in header_str.lines() {
                    if let Some((k, v)) = line.split_once(':') {
                        if k.trim().eq_ignore_ascii_case("content-length") {
                            content_length = v.trim().parse::<usize>().unwrap_or(0);
                        }
                    }
                }
            }
        }
        if let Some(h_end) = header_end {
            if buffer.len() >= h_end + content_length {
                break;
            }
        }
    }

    let Some(h_end) = header_end else {
        return Ok(());
    };

    let header_str = String::from_utf8_lossy(&buffer[..h_end]);
    let first_line = header_str.lines().next().unwrap_or("");
    let mut parts = first_line.split_whitespace();
    let method = parts.next().unwrap_or("GET");
    let raw_path = parts.next().unwrap_or("/");
    let path = raw_path.split('?').next().unwrap_or("/");

    let body_bytes = if buffer.len() >= h_end + content_length {
        &buffer[h_end..h_end + content_length]
    } else {
        &buffer[h_end..]
    };
    let body_str = String::from_utf8_lossy(body_bytes);

    if method.eq_ignore_ascii_case("GET") {
        handle_get(&mut stream, path, &state)
    } else if method.eq_ignore_ascii_case("POST") {
        handle_post(&mut stream, path, &body_str, &state)
    } else {
        send_json_response(&mut stream, 405, &json!({"error": "Method Not Allowed"}))
    }
}

fn handle_get(
    stream: &mut TcpStream,
    path: &str,
    state: &Arc<Mutex<StudioServerState>>,
) -> Result<(), std::io::Error> {
    let mut guard = match state.lock() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    };

    if path == "/" || path == "/index.html" {
        let candidates = [
            guard.tuner.root.join("scripts").join("web").join("index.html"),
            guard.tuner.root.join("web").join("index.html"),
        ];
        for index_file in &candidates {
            if index_file.is_file() {
                if let Ok(bytes) = fs::read(index_file) {
                    let header = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nCache-Control: no-cache, no-store, must-revalidate\r\nConnection: close\r\n\r\n",
                        bytes.len()
                    );
                    stream.write_all(header.as_bytes())?;
                    stream.write_all(&bytes)?;
                    return Ok(());
                }
            }
        }
        return send_json_response(stream, 404, &json!({"error": "index.html not found"}));
    }

    if path == "/api/revisions" {
        let recs = guard.tuner.load_revisions();
        return send_json_response(stream, 200, &Value::Array(recs));
    }

    if path == "/api/benchmark" {
        let res = guard.tuner.verify_golden_benchmark();
        return send_json_response(stream, 200, &res);
    }

    if path == "/api/tailcat/proposal" {
        let sample_rules = vec![
            json!({"prev": "讓", "next": "不懂", "score": 26.0, "kind": "collocation"}),
            json!({"prev": "不懂", "next": "程式", "score": 26.0, "kind": "collocation"}),
        ];
        let pkg = guard.tuner.package_tailcat_proposal(&sample_rules);
        return send_json_response(stream, 200, &pkg);
    }

    if path == "/api/ai/status" {
        let mut conn = guard.consultant.test_connection();
        let eff_provider = guard.consultant.get_effective_provider();
        let wizard_done = guard.consultant.get_bool("ai_wizard_completed", false);
        let strategy = guard.consultant.get_str("ai_strategy", "remote_first");
        let offline_models = guard.consultant.read_offline_models();
        let ollama_paths = guard.consultant.find_ollama_paths();
        let has_key = guard.consultant.has_gemini_key();
        let ai_prov = guard.consultant.get_str("ai_provider", "auto");
        let gem_model = guard.consultant.get_str("gemini_model", "gemini-2.5-flash");
        let oll_url = guard
            .consultant
            .get_str("ollama_url", "http://127.0.0.1:11434");
        let oll_model = guard.consultant.get_str("ollama_model", "");

        if let Some(obj) = conn.as_object_mut() {
            obj.insert("provider".to_string(), json!(eff_provider));
            obj.insert("wizard_completed".to_string(), json!(wizard_done));
            obj.insert("strategy".to_string(), json!(strategy));
            obj.insert("offline_models".to_string(), json!(offline_models));
            obj.insert("ollama_paths".to_string(), ollama_paths);
            obj.insert(
                "config".to_string(),
                json!({
                    "ai_provider": ai_prov,
                    "has_gemini_key": has_key,
                    "gemini_model": gem_model,
                    "ollama_url": oll_url,
                    "ollama_model": oll_model
                }),
            );
        }
        return send_json_response(stream, 200, &conn);
    }

    if path == "/api/ai/wizard_status" {
        let ws = guard.consultant.get_ai_wizard_status();
        return send_json_response(stream, 200, &ws);
    }

    if path == "/api/ai/local_recommendations" {
        let recs = guard.consultant.check_local_model_recommendations();
        return send_json_response(stream, 200, &recs);
    }

    if path == "/api/ai/handover_spec" {
        let StudioServerState {
            ref tuner,
            ref mut consultant,
            ..
        } = *guard;
        let spec = consultant.get_recommended_models_and_handover_spec(tuner);
        return send_json_response(stream, 200, &spec);
    }

    if path == "/api/rules/inventory" {
        let inv = guard.tuner.get_rule_inventory_summary();
        return send_json_response(stream, 200, &inv);
    }

    if path == "/api/tune/preload" {
        let recent = guard.tuner.scan_recent_txt_files();
        let preload_target = guard.preloaded_file.clone();
        let mut res = json!({
            "status": "ok",
            "has_preload": false,
            "file_path": "",
            "pairs": [],
            "recent_files": recent
        });
        if !preload_target.is_empty() {
            if let Some(p) = guard
                .tuner
                .resolve_local_txt_file(&preload_target, &preload_target)
            {
                let pairs = guard.tuner.load_pairs_from_file(&p);
                res = json!({
                    "status": "ok",
                    "has_preload": true,
                    "file_path": p.to_string_lossy(),
                    "file_name": p.file_name().and_then(|s| s.to_str()).unwrap_or("語意分析.txt"),
                    "pairs": pairs,
                    "recent_files": recent
                });
            }
        }
        return send_json_response(stream, 200, &res);
    }

    send_json_response(stream, 404, &json!({"error": "Not Found"}))
}

fn handle_post(
    stream: &mut TcpStream,
    path: &str,
    body_str: &str,
    state: &Arc<Mutex<StudioServerState>>,
) -> Result<(), std::io::Error> {
    let mut guard = match state.lock() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    };

    let req_data: Value = serde_json::from_str(body_str).unwrap_or_else(|_| json!({}));

    if path == "/api/tune" {
        let mut pairs = req_data
            .get("pairs")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        if pairs.is_empty() {
            let err = req_data.get("error").and_then(Value::as_str).unwrap_or("").trim();
            let cor = req_data.get("correct").and_then(Value::as_str).unwrap_or("").trim();
            if !err.is_empty() || !cor.is_empty() {
                pairs.push(json!({"error": err, "correct": cor}));
            }
        }

        let mut all_diffs = Vec::new();
        let mut all_rules = Vec::new();
        let mut all_constructions = Vec::new();
        let mut oov_words_to_add = Vec::new();
        let mut linguistic_analyses = Vec::new();
        let mut seen_words = std::collections::HashSet::new();
        let mut seen_patterns = std::collections::HashSet::new();
        let mut seen_rule_keys = std::collections::HashSet::new();

        for p in &pairs {
            let e = p.get("error").and_then(Value::as_str).unwrap_or("").trim();
            let c = p.get("correct").and_then(Value::as_str).unwrap_or("").trim();
            if e.is_empty() || c.is_empty() {
                continue;
            }
            let custom_err: Option<Vec<String>> = p
                .get("error_tokens")
                .and_then(Value::as_array)
                .map(|a| a.iter().filter_map(Value::as_str).map(String::from).collect());
            let custom_cor: Option<Vec<String>> = p
                .get("correct_tokens")
                .and_then(Value::as_array)
                .map(|a| a.iter().filter_map(Value::as_str).map(String::from).collect());

            let diffs = guard.tuner.align_sentence_pair(
                e,
                c,
                custom_err.as_deref(),
                custom_cor.as_deref(),
            );
            for d in &diffs {
                if let Some(ws) = d.get("oov_compound_words").and_then(Value::as_array) {
                    for w in ws {
                        let w_key = w
                            .get("word")
                            .and_then(Value::as_str)
                            .or_else(|| w.as_str())
                            .unwrap_or("")
                            .to_string();
                        if !w_key.is_empty() && seen_words.insert(w_key) {
                            oov_words_to_add.push(w.clone());
                        }
                    }
                }
                if let Some(cs) = d.get("hierarchical_constructions").and_then(Value::as_array) {
                    for c_obj in cs {
                        let pid = c_obj
                            .get("pattern_id")
                            .or_else(|| c_obj.get("formula"))
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .to_string();
                        if !pid.is_empty() && seen_patterns.insert(pid) {
                            all_constructions.push(c_obj.clone());
                        }
                        if let Some(brs) = c_obj.get("backbone_rules").and_then(Value::as_array) {
                            for br in brs {
                                let rp = br.get("prev").and_then(Value::as_str).unwrap_or("").to_string();
                                let rn = br.get("next").and_then(Value::as_str).unwrap_or("").to_string();
                                if !rp.is_empty() && !rn.is_empty() && seen_rule_keys.insert((rp, rn)) {
                                    all_rules.push(br.clone());
                                }
                            }
                        }
                    }
                }
                for r in guard.tuner.synthesize_rules(d) {
                    let rp = r.get("prev").and_then(Value::as_str).unwrap_or("").to_string();
                    let rn = r.get("next").and_then(Value::as_str).unwrap_or("").to_string();
                    if !rp.is_empty() && !rn.is_empty() && seen_rule_keys.insert((rp, rn)) {
                        all_rules.push(r);
                    }
                }
                let rej = d.get("rejected").and_then(Value::as_str).unwrap_or("");
                let acc = d.get("accepted").and_then(Value::as_str).unwrap_or("");
                let prev = d.get("prev_ctx").and_then(Value::as_str).unwrap_or("");
                let next = d.get("next_ctx").and_then(Value::as_str).unwrap_or("");
                linguistic_analyses.push(
                    guard
                        .tuner
                        .analyze_linguistic_context(rej, acc, prev, next, Some(d)),
                );
            }
            all_diffs.extend(diffs);
        }

        // 執行全詞庫「舉一反三」同構掃描，將同類跨界陷阱、派生詞族與專名一併納入
        let analog_scan = guard.tuner.scan_analogous_cases_across_lexicon(&all_diffs);
        let analogical_families = analog_scan
            .get("scanned_families")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        if let Some(aw_list) = analog_scan.get("analogous_words").and_then(Value::as_array) {
            for w in aw_list {
                let w_key = w
                    .get("word")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                if !w_key.is_empty() && seen_words.insert(w_key) {
                    oov_words_to_add.push(w.clone());
                }
            }
        }
        if let Some(ac_list) = analog_scan.get("analogous_constructions").and_then(Value::as_array) {
            for c_obj in ac_list {
                let pid = c_obj
                    .get("pattern_id")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                if !pid.is_empty() && seen_patterns.insert(pid) {
                    all_constructions.push(c_obj.clone());
                }
                if let Some(brs) = c_obj.get("backbone_rules").and_then(Value::as_array) {
                    for br in brs {
                        let rp = br.get("prev").and_then(Value::as_str).unwrap_or("").to_string();
                        let rn = br.get("next").and_then(Value::as_str).unwrap_or("").to_string();
                        if !rp.is_empty() && !rn.is_empty() && seen_rule_keys.insert((rp, rn)) {
                            all_rules.push(br.clone());
                        }
                    }
                }
            }
        }

        let factorized_arrays = guard.tuner.factorize_rules_to_arrays(&all_rules);
        for fa in &factorized_arrays {
            let pid = fa
                .get("pattern_id")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            if !pid.is_empty() && seen_patterns.insert(pid) {
                all_constructions.push(fa.clone());
            }
        }

        let upsert_rep = guard.tuner.upsert_patterns_with_report(&all_constructions);
        let patterns_created = upsert_rep.get("added_count").and_then(Value::as_u64).unwrap_or(0) as usize;
        let patterns_updated = upsert_rep.get("updated_count").and_then(Value::as_u64).unwrap_or(0) as usize;
        let words_added = guard.tuner.append_words_to_domain_lexicon(&oov_words_to_add);
        let added = guard.tuner.append_rules_to_tsv(&all_rules);

        let resp = json!({
            "status": "ok",
            "added_count": added,
            "words_added_count": words_added,
            "patterns_added_count": patterns_created + patterns_updated,
            "patterns_created_count": patterns_created,
            "patterns_updated_count": patterns_updated,
            "augmented_rules": upsert_rep.get("augmented_rules").cloned().unwrap_or_else(|| json!([])),
            "created_rules": upsert_rep.get("created_rules").cloned().unwrap_or_else(|| json!([])),
            "factorized_arrays": factorized_arrays,
            "analogical_families": analogical_families,
            "added_words": oov_words_to_add,
            "constructions": all_constructions,
            "pairs_count": pairs.len(),
            "diffs": all_diffs,
            "rules": all_rules,
            "linguistic_analyses": linguistic_analyses
        });
        return send_json_response(stream, 200, &resp);
    }

    if path == "/api/tune/tokenize" {
        let text = req_data.get("text").and_then(Value::as_str).unwrap_or("");
        let error = req_data.get("error").and_then(Value::as_str).unwrap_or("");
        let correct = req_data.get("correct").and_then(Value::as_str).unwrap_or("");
        let mut res = json!({"status": "ok"});
        if !text.is_empty() {
            let toks: Vec<String> = guard
                .tuner
                .tokenize_with_spans(text)
                .into_iter()
                .map(|(t, _, _)| t)
                .collect();
            res["tokens"] = json!(toks);
        }
        if !error.is_empty() && !correct.is_empty() {
            let (e_sp, c_sp) = guard.tuner.tokenize_pair_with_spans(error, correct);
            res["error_tokens"] = json!(e_sp.into_iter().map(|(t, _, _)| t).collect::<Vec<_>>());
            res["correct_tokens"] = json!(c_sp.into_iter().map(|(t, _, _)| t).collect::<Vec<_>>());
        } else {
            if !error.is_empty() {
                res["error_tokens"] = json!(guard
                    .tuner
                    .tokenize_with_spans(error)
                    .into_iter()
                    .map(|(t, _, _)| t)
                    .collect::<Vec<_>>());
            }
            if !correct.is_empty() {
                res["correct_tokens"] = json!(guard
                    .tuner
                    .tokenize_with_spans(correct)
                    .into_iter()
                    .map(|(t, _, _)| t)
                    .collect::<Vec<_>>());
            }
        }
        return send_json_response(stream, 200, &res);
    }

    if path == "/api/revisions/clear" {
        let _ = guard.tuner.clear_revisions();
        return send_json_response(
            stream,
            200,
            &json!({"status": "ok", "message": "已成功清空所有修訂歷史！"}),
        );
    }

    if path == "/api/revisions/sublimate" {
        let StudioServerState {
            ref mut tuner,
            ref mut consultant,
            ..
        } = *guard;
        let res = consultant.sublimate_revisions(tuner);
        return send_json_response(stream, 200, &res);
    }

    if path == "/api/ai/config" {
        let provider = req_data
            .get("provider")
            .and_then(Value::as_str)
            .unwrap_or("auto");
        let gemini_api_key = req_data
            .get("gemini_api_key")
            .and_then(Value::as_str)
            .unwrap_or("");
        let gemini_model = req_data
            .get("gemini_model")
            .and_then(Value::as_str)
            .unwrap_or("gemini-2.5-flash");
        let ollama_url = req_data
            .get("ollama_url")
            .and_then(Value::as_str)
            .unwrap_or("http://127.0.0.1:11434");
        let ollama_model = req_data
            .get("ollama_model")
            .and_then(Value::as_str)
            .unwrap_or("gemma2");
        let openai_api_key = req_data
            .get("openai_api_key")
            .and_then(Value::as_str)
            .unwrap_or("");
        let openai_model = req_data
            .get("openai_model")
            .and_then(Value::as_str)
            .unwrap_or("gpt-4o-mini");
        let claude_api_key = req_data
            .get("claude_api_key")
            .and_then(Value::as_str)
            .unwrap_or("");
        let claude_model = req_data
            .get("claude_model")
            .and_then(Value::as_str)
            .unwrap_or("claude-3-5-sonnet-20241022");
        let custom_base_url = req_data
            .get("custom_base_url")
            .and_then(Value::as_str)
            .unwrap_or("https://api.openai.com/v1");
        let custom_api_key = req_data
            .get("custom_api_key")
            .and_then(Value::as_str)
            .unwrap_or("");
        let custom_model = req_data
            .get("custom_model")
            .and_then(Value::as_str)
            .unwrap_or("");

        guard.consultant.save_ai_settings(
            provider,
            gemini_api_key,
            gemini_model,
            ollama_url,
            ollama_model,
            openai_api_key,
            openai_model,
            claude_api_key,
            claude_model,
            custom_base_url,
            custom_api_key,
            custom_model,
        );
        let mut test_res = guard.consultant.test_connection();
        let eff = guard.consultant.get_effective_provider();
        test_res["provider"] = json!(eff);
        return send_json_response(stream, 200, &test_res);
    }

    if path == "/api/ai/wizard_save" {
        let strategy = req_data
            .get("strategy")
            .and_then(Value::as_str)
            .unwrap_or("remote_first");
        let gemini_key = req_data
            .get("gemini_api_key")
            .and_then(Value::as_str)
            .unwrap_or("");
        let gemini_model = req_data
            .get("gemini_model")
            .and_then(Value::as_str)
            .unwrap_or("gemini-2.5-flash");
        let ollama_url = req_data
            .get("ollama_url")
            .and_then(Value::as_str)
            .unwrap_or("http://127.0.0.1:11434");
        let ollama_model = req_data
            .get("ollama_model")
            .and_then(Value::as_str)
            .unwrap_or("");
        let remote_provider = req_data
            .get("remote_provider")
            .and_then(Value::as_str)
            .unwrap_or("gemini");
        let openai_api_key = req_data
            .get("openai_api_key")
            .and_then(Value::as_str)
            .unwrap_or("");
        let openai_model = req_data
            .get("openai_model")
            .and_then(Value::as_str)
            .unwrap_or("gpt-4o-mini");
        let claude_api_key = req_data
            .get("claude_api_key")
            .and_then(Value::as_str)
            .unwrap_or("");
        let claude_model = req_data
            .get("claude_model")
            .and_then(Value::as_str)
            .unwrap_or("claude-3-5-sonnet-20241022");
        let custom_base_url = req_data
            .get("custom_base_url")
            .and_then(Value::as_str)
            .unwrap_or("https://api.openai.com/v1");
        let custom_api_key = req_data
            .get("custom_api_key")
            .and_then(Value::as_str)
            .unwrap_or("");
        let custom_model = req_data
            .get("custom_model")
            .and_then(Value::as_str)
            .unwrap_or("");

        let ok = guard.consultant.save_ai_wizard_strategy(
            strategy,
            gemini_key,
            gemini_model,
            ollama_url,
            ollama_model,
            remote_provider,
            openai_api_key,
            openai_model,
            claude_api_key,
            claude_model,
            custom_base_url,
            custom_api_key,
            custom_model,
        );
        let mut test_res = guard.consultant.test_connection();
        test_res["status"] = json!(if ok { "ok" } else { "error" });
        test_res["strategy"] = json!(strategy);
        return send_json_response(stream, 200, &test_res);
    }

    if path == "/api/ai/start_ollama" {
        let started = guard.consultant.start_ollama_service();
        let mut test_res = guard.consultant.test_connection();
        test_res["started"] = json!(started);
        return send_json_response(stream, 200, &test_res);
    }

    if path == "/api/ai/diagnose" {
        let mut pairs = req_data
            .get("pairs")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        if pairs.is_empty() {
            let err = req_data.get("error").and_then(Value::as_str).unwrap_or("").trim();
            let cor = req_data.get("correct").and_then(Value::as_str).unwrap_or("").trim();
            if !err.is_empty() || !cor.is_empty() {
                pairs.push(json!({"error": err, "correct": cor}));
            }
        }

        let StudioServerState {
            ref mut tuner,
            ref mut consultant,
            ..
        } = *guard;

        for p in &mut pairs {
            let e = p.get("error").and_then(Value::as_str).unwrap_or("").trim().to_string();
            let c = p.get("correct").and_then(Value::as_str).unwrap_or("").trim().to_string();
            let custom_err: Option<Vec<String>> = p
                .get("error_tokens")
                .and_then(Value::as_array)
                .map(|a| a.iter().filter_map(Value::as_str).map(String::from).collect());
            let custom_cor: Option<Vec<String>> = p
                .get("correct_tokens")
                .and_then(Value::as_array)
                .map(|a| a.iter().filter_map(Value::as_str).map(String::from).collect());
            if !e.is_empty() && !c.is_empty() {
                let diffs = tuner.align_sentence_pair(
                    &e,
                    &c,
                    custom_err.as_deref(),
                    custom_cor.as_deref(),
                );
                p["symbolic_diffs"] = json!(diffs);
            }
        }

        let mut diag = consultant.diagnose_sentence_pairs_batch(&pairs, tuner);
        let mut words_to_persist = Vec::new();
        let mut constructions_to_persist = Vec::new();
        let mut rules_to_persist = Vec::new();

        if let Some(items) = diag.get("items").and_then(Value::as_array) {
            for it in items {
                if it.get("is_user_typo").and_then(Value::as_bool).unwrap_or(false) {
                    continue;
                }
                if let Some(ws) = it.get("recommended_lexicon_words").and_then(Value::as_array) {
                    words_to_persist.extend(ws.clone());
                }
                if let Some(cs) = it.get("recommended_constructions").and_then(Value::as_array) {
                    constructions_to_persist.extend(cs.clone());
                }
                if let Some(rs) = it.get("recommended_rules").and_then(Value::as_array) {
                    rules_to_persist.extend(rs.clone());
                }
            }
        }

        let auto_patterns = tuner.append_patterns_to_jsonl(&constructions_to_persist);
        let auto_words = tuner.append_words_to_domain_lexicon(&words_to_persist);
        let auto_rules = tuner.append_rules_to_tsv(&rules_to_persist);
        diag["auto_added_patterns_count"] = json!(auto_patterns);
        diag["auto_added_words_count"] = json!(auto_words);
        diag["auto_added_rules_count"] = json!(auto_rules);
        return send_json_response(stream, 200, &diag);
    }

    if path == "/api/ai/extract_corpus" {
        let text = req_data.get("text").and_then(Value::as_str).unwrap_or("").trim();
        let domain = req_data
            .get("domain")
            .and_then(Value::as_str)
            .unwrap_or("通用專業")
            .trim();
        let res = guard
            .consultant
            .extract_domain_corpus(text, domain, &guard.tuner);
        return send_json_response(stream, 200, &res);
    }

    if path == "/api/ai/import_extracted" {
        let raw_terms = req_data
            .get("terms")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let raw_transitions = req_data
            .get("transitions")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();

        let words_added = guard.tuner.append_words_to_domain_lexicon(&raw_terms);
        let mut rules_to_add = Vec::new();
        for t in &raw_transitions {
            let prev = t.get("prev").and_then(Value::as_str).unwrap_or("").trim();
            let next = t.get("next").and_then(Value::as_str).unwrap_or("").trim();
            let score = t.get("score").and_then(Value::as_f64).unwrap_or(28.0);
            let desc = t
                .get("description")
                .and_then(Value::as_str)
                .unwrap_or("AI文獻萃取領域搭配");
            if !prev.is_empty() && !next.is_empty() && score > 0.0 {
                rules_to_add.push(json!({
                    "prev": prev,
                    "next": next,
                    "score": score,
                    "kind": "collocation",
                    "comment": desc
                }));
            }
        }
        let added = guard.tuner.append_rules_to_tsv(&rules_to_add);
        return send_json_response(
            stream,
            200,
            &json!({"status": "ok", "added_count": added, "words_added_count": words_added}),
        );
    }

    if path == "/api/ai/audit_proposal" {
        let proposal = req_data.get("proposal").cloned().unwrap_or_else(|| json!({}));
        let res = guard.consultant.audit_tailcat_proposal(&proposal);
        return send_json_response(stream, 200, &res);
    }

    if path == "/api/tailcat/import" {
        let pkg = req_data
            .get("package")
            .cloned()
            .unwrap_or_else(|| req_data.clone());
        let res = guard.tuner.import_tailcat_package(&pkg);
        return send_json_response(stream, 200, &res);
    }

    if path == "/api/tailcat/pull_github" {
        let feed_url = req_data
            .get("feed_url")
            .and_then(Value::as_str)
            .unwrap_or("");
        let force = req_data
            .get("force_refresh")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let res = guard.tuner.pull_community_rules_with_etag(feed_url, force);
        return send_json_response(stream, 200, &res);
    }

    if path == "/api/ai/set_prompt_preference" {
        let skip = req_data
            .get("skip_prompt")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let ok = guard.consultant.set_local_model_prompt_preference(skip);
        return send_json_response(
            stream,
            200,
            &json!({"status": if ok { "ok" } else { "error" }, "skip_prompt": skip}),
        );
    }

    if path == "/api/ai/quick_auth_gemini" {
        let env_k = std::env::var("GEMINI_API_KEY").unwrap_or_default();
        let cfg_k = guard.consultant.get_str("gemini_api_key", "");
        let found = if !env_k.trim().is_empty() {
            env_k.trim().to_string()
        } else {
            cfg_k.trim().to_string()
        };

        if !found.is_empty() {
            guard
                .consultant
                .config
                .insert("gemini_api_key".to_string(), json!(found));
            let _ = guard.consultant.save_config();
            return send_json_response(
                stream,
                200,
                &json!({
                    "found_local": true,
                    "key_masked": "***",
                    "message": "已成功自本機環境偵測並自動套用 Gemini API Key！"
                }),
            );
        } else {
            let _ = Command::new("cmd.exe")
                .args(["/c", "start", "", "https://aistudio.google.com/app/apikey"])
                .spawn();
            return send_json_response(
                stream,
                200,
                &json!({
                    "found_local": false,
                    "browser_opened": true,
                    "message": "已在瀏覽器開啟 Google AI Studio。登入後點擊【Create API key】並複製，切回本視窗將自動識別！"
                }),
            );
        }
    }

    if path == "/api/tune/load_file" {
        let target_path = req_data.get("path").and_then(Value::as_str).unwrap_or("");
        let target_name = req_data
            .get("file_name")
            .and_then(Value::as_str)
            .unwrap_or("");
        if let Some(p) = guard.tuner.resolve_local_txt_file(target_path, target_name) {
            let pairs = guard.tuner.load_pairs_from_file(&p);
            let mtime = p
                .metadata()
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs())
                .unwrap_or(0);
            return send_json_response(
                stream,
                200,
                &json!({
                    "status": "ok",
                    "file_path": p.to_string_lossy(),
                    "file_name": p.file_name().and_then(|s| s.to_str()).unwrap_or("語意分析.txt"),
                    "pairs": pairs,
                    "count": pairs.len(),
                    "mtime": mtime
                }),
            );
        } else {
            let missing = if !target_path.is_empty() {
                target_path
            } else if !target_name.is_empty() {
                target_name
            } else {
                "語意分析.txt"
            };
            return send_json_response(
                stream,
                404,
                &json!({"status": "error", "message": format!("找不到指定檔案：{missing}")}),
            );
        }
    }

    if path == "/api/ai/install_green_engine" {
        let mut script_path = guard
            .tuner
            .root
            .join("scripts")
            .join("安裝本地AI純綠推論引擎.ps1");
        if !script_path.is_file() {
            script_path = guard.tuner.root.join("安裝本地AI純綠推論引擎.ps1");
        }
        let _ = Command::new("cmd.exe")
            .args([
                "/c",
                "start",
                "RuhI 本地純綠 AI 推論引擎安裝精靈",
                "powershell.exe",
                "-NoExit",
                "-ExecutionPolicy",
                "Bypass",
                "-File",
                &script_path.to_string_lossy(),
            ])
            .spawn();
        return send_json_response(
            stream,
            200,
            &json!({"status": "ok", "message": "已在桌面喚起 PowerShell 純綠推論引擎安裝精靈視窗！"}),
        );
    }

    send_json_response(stream, 404, &json!({"error": "Not Found"}))
}

fn send_json_response(
    stream: &mut TcpStream,
    status_code: u16,
    payload: &Value,
) -> Result<(), std::io::Error> {
    let status_text = match status_code {
        200 => "OK",
        404 => "Not Found",
        405 => "Method Not Allowed",
        _ => "Internal Server Error",
    };
    let body = serde_json::to_vec(payload).unwrap_or_else(|_| b"{}".to_vec());
    let header = format!(
        "HTTP/1.1 {status_code} {status_text}\r\nContent-Type: application/json; charset=utf-8\r\nContent-Length: {}\r\nAccess-Control-Allow-Origin: *\r\nConnection: close\r\n\r\n",
        body.len()
    );
    stream.write_all(header.as_bytes())?;
    stream.write_all(&body)?;
    Ok(())
}

fn find_subsequence(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}
