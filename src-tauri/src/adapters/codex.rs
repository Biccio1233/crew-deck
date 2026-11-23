//! Codex: `~/.codex/sessions/AAAA/MM/GG/rollout-*.jsonl`.
use crate::model::{azione_strumento, corto, ts_ms, Acc, Ev, Token};
use serde_json::Value;

pub fn feed(acc: &mut Acc, riga: &Value) {
    let tipo = riga.get("type").and_then(|v| v.as_str()).unwrap_or("");
    let p = riga.get("payload").cloned().unwrap_or(Value::Null);
    let sotto = p.get("type").and_then(|v| v.as_str()).unwrap_or("");
    let Some(ts) = ts_ms(riga.get("timestamp")) else { return };
    match (tipo, sotto) {
        ("session_meta", _) => {
            if let Some(id) = p.get("id").and_then(|v| v.as_str()) {
                acc.id = id.to_string();
            }
            if let Some(c) = p.get("cwd").and_then(|v| v.as_str()) {
                acc.cwd = Some(c.to_string());
            }
            if let Some(o) = p.get("originator").and_then(|v| v.as_str()) {
                acc.origine = Some(o.to_string());
                acc.automatica = o.contains("exec") || o.contains("sdk");
            }
            acc.tocca(ts);
        }
        ("turn_context", _) => {
            if let Some(c) = p.get("cwd").and_then(|v| v.as_str()) {
                acc.cwd = Some(c.to_string());
            }
            if let Some(m) = p.get("model").and_then(|v| v.as_str()) {
                acc.modello = Some(m.to_string());
            }
            acc.tocca(ts);
        }
        ("event_msg", "thread_settings_applied") => {
            if let Some(m) = p.pointer("/thread_settings/model").and_then(|v| v.as_str()) {
                acc.modello = Some(m.to_string());
            }
            acc.tocca(ts);
        }
        ("event_msg", "task_started") => {
            acc.inizio_turno = Some(ts);
            acc.evento(Ev::Pensa, ts);
        }
        ("event_msg", "user_message") => {
            let t = p.get("message").and_then(|v| v.as_str()).unwrap_or("");
            if !t.trim().is_empty() {
                acc.prompt(corto(t, 300), ts);
                if acc.titolo.is_none() {
                    acc.titolo = Some(corto(t, 60));
                }
            }
        }
        ("event_msg", "task_complete") => acc.fine_turno(ts),
        ("event_msg", "turn_aborted") => {
            acc.aperti.clear();
            acc.evento(Ev::Interrotto, ts);
        }
        ("event_msg", "error") => {
            acc.errori += 1;
            acc.evento(Ev::ErroreApi, ts);
        }
        ("event_msg", "token_count") => {
            // totale cumulativo della sessione: si sostituisce, non si somma
            if let Some(u) = p.pointer("/info/total_token_usage") {
                let totale = u.get("total_tokens").and_then(|v| v.as_u64()).unwrap_or(0);
                acc.token = Token {
                    input: u.get("input_tokens").and_then(|v| v.as_u64()).unwrap_or(0),
                    output: u.get("output_tokens").and_then(|v| v.as_u64()).unwrap_or(0),
                    cache_lettura: u.get("cached_input_tokens").and_then(|v| v.as_u64()).unwrap_or(0),
                    cache_scrittura: u.get("cache_write_input_tokens").and_then(|v| v.as_u64()).unwrap_or(0),
                };
                // l'app desktop scrive solo il totale: lo si conta come input per non mostrare zero
                let parti = acc.token.input + acc.token.output + acc.token.cache_lettura + acc.token.cache_scrittura;
                if parti == 0 && totale > 0 {
                    acc.token.input = totale;
                }
            }
            acc.tocca(ts);
        }
        ("response_item", "function_call") | ("response_item", "custom_tool_call") => {
            let id = p.get("call_id").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let nome = p.get("name").and_then(|v| v.as_str()).unwrap_or("?").to_string();
            let args = p
                .get("arguments")
                .and_then(|v| v.as_str())
                .and_then(|s| serde_json::from_str::<Value>(s).ok())
                .or_else(|| p.get("input").cloned())
                .unwrap_or(Value::Null);
            let azione = match p.get("input").and_then(|v| v.as_str()) {
                // Codex scrive i comandi dentro piccoli script: si estrae il `cmd`
                Some(script) => match cmd_da_script(script) {
                    Some(c) => format!("Esegue: {}", corto(&c, 60)),
                    None => "Esegue uno script di strumenti".to_string(),
                },
                None => azione_strumento(&nome, &args),
            };
            acc.apri(id, nome, azione, ts);
        }
        ("response_item", "function_call_output") | ("response_item", "custom_tool_call_output") => {
            let id = p.get("call_id").and_then(|v| v.as_str()).unwrap_or("");
            acc.chiudi(id, false, ts);
        }
        ("response_item", "reasoning") => acc.evento(Ev::Pensa, ts),
        ("response_item", "message") => {
            if p.get("role").and_then(|v| v.as_str()) == Some("assistant") {
                acc.evento(Ev::Testo, ts);
            } else {
                acc.tocca(ts);
            }
        }
        _ => acc.tocca(ts),
    }
}

/// Estrae il primo `cmd:"…"` da uno script di strumenti di Codex.
pub fn cmd_da_script(script: &str) -> Option<String> {
    let i = script.find("cmd:\"")? + 5;
    let mut out = String::new();
    let mut esc = false;
    for ch in script[i..].chars() {
        if esc {
            out.push(match ch { 'n' => ' ', 't' => ' ', c => c });
            esc = false;
        } else if ch == '\\' {
            esc = true;
        } else if ch == '"' {
            return Some(out);
        } else {
            out.push(ch);
        }
    }
    None
}
