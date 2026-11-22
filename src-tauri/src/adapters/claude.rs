//! Claude Code: `~/.claude/projects/<cartella>/<sessione>.jsonl`.
use crate::model::{azione_strumento, corto, ts_ms, Acc, Ev, Token};
use serde_json::Value;

pub fn feed(acc: &mut Acc, riga: &Value) {
    let tipo = riga.get("type").and_then(|v| v.as_str()).unwrap_or("");
    if riga.get("isSidechain").and_then(|v| v.as_bool()) == Some(true) {
        return; // i sotto-agenti hanno file propri
    }
    if let Some(c) = riga.get("cwd").and_then(|v| v.as_str()) {
        if acc.cwd.is_none() {
            acc.cwd = Some(c.to_string());
        }
    }
    if acc.origine.is_none() {
        if let Some(e) = riga.get("entrypoint").and_then(|v| v.as_str()) {
            acc.origine = Some(e.to_string());
            acc.automatica = e.starts_with("sdk");
        }
    }
    let ts = ts_ms(riga.get("timestamp"));
    match tipo {
        "ai-title" => {
            if let Some(t) = riga.get("aiTitle").and_then(|v| v.as_str()) {
                acc.titolo = Some(t.to_string());
            }
        }
        "user" => {
            let Some(ts) = ts else { return };
            if riga.get("isMeta").and_then(|v| v.as_bool()) == Some(true) {
                return;
            }
            let msg = riga.get("message").cloned().unwrap_or(Value::Null);
            match msg.get("content") {
                Some(Value::String(s)) => testo_utente(acc, s, ts),
                Some(Value::Array(parti)) => {
                    let mut testo = String::new();
                    for p in parti {
                        match p.get("type").and_then(|v| v.as_str()) {
                            Some("tool_result") => {
                                let id = p.get("tool_use_id").and_then(|v| v.as_str()).unwrap_or("");
                                let err = p.get("is_error").and_then(|v| v.as_bool()).unwrap_or(false);
                                acc.chiudi(id, err, ts);
                            }
                            Some("text") => {
                                testo.push_str(p.get("text").and_then(|v| v.as_str()).unwrap_or(""));
                            }
                            _ => {}
                        }
                    }
                    if !testo.is_empty() {
                        testo_utente(acc, &testo, ts);
                    }
                }
                _ => {}
            }
        }
        "assistant" => {
            let Some(ts) = ts else { return };
            let msg = riga.get("message").cloned().unwrap_or(Value::Null);
            if let Some(m) = msg.get("model").and_then(|v| v.as_str()) {
                if !m.starts_with('<') {
                    acc.modello = Some(m.to_string());
                }
            }
            if let Some(u) = msg.get("usage") {
                let t = Token {
                    input: u.get("input_tokens").and_then(|v| v.as_u64()).unwrap_or(0),
                    output: u.get("output_tokens").and_then(|v| v.as_u64()).unwrap_or(0),
                    cache_lettura: u.get("cache_read_input_tokens").and_then(|v| v.as_u64()).unwrap_or(0),
                    cache_scrittura: u.get("cache_creation_input_tokens").and_then(|v| v.as_u64()).unwrap_or(0),
                };
                acc.usage(msg.get("id").and_then(|v| v.as_str()), t);
            }
            if riga.get("isApiErrorMessage").and_then(|v| v.as_bool()) == Some(true) {
                acc.errori += 1;
                acc.evento(Ev::ErroreApi, ts);
                return;
            }
            let mut ultimo = Ev::Testo;
            if let Some(Value::Array(parti)) = msg.get("content") {
                for p in parti {
                    match p.get("type").and_then(|v| v.as_str()) {
                        Some("tool_use") => {
                            let id = p.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
                            let nome = p.get("name").and_then(|v| v.as_str()).unwrap_or("?").to_string();
                            let input = p.get("input").cloned().unwrap_or(Value::Null);
                            let azione = azione_strumento(&nome, &input);
                            acc.apri(id, nome, azione, ts);
                            ultimo = Ev::Strumento;
                        }
                        Some("thinking") | Some("redacted_thinking") => ultimo = Ev::Pensa,
                        Some("text") => ultimo = Ev::Testo,
                        _ => {}
                    }
                }
            }
            if ultimo != Ev::Strumento {
                acc.evento(ultimo, ts);
            }
            if msg.get("stop_reason").and_then(|v| v.as_str()) == Some("end_turn") && acc.aperti.is_empty() {
                acc.fine_turno(ts);
            }
        }
        "system" => {
            let Some(ts) = ts else { return };
            match riga.get("subtype").and_then(|v| v.as_str()) {
                Some("turn_duration") => acc.fine_turno(ts),
                Some("api_error") => {
                    acc.errori += 1;
                    acc.evento(Ev::ErroreApi, ts);
                }
                _ => acc.tocca(ts),
            }
        }
        _ => {
            if let Some(ts) = ts {
                acc.tocca(ts);
            }
        }
    }
}

fn testo_utente(acc: &mut Acc, s: &str, ts: i64) {
    let basso = s.to_lowercase();
    if basso.contains("[request interrupted by user") {
        acc.aperti.clear();
        acc.domanda_aperta = false;
        acc.evento(Ev::Interrotto, ts);
        return;
    }
    // comandi locali e messaggi di sistema iniettati: non sono richieste dell'utente
    if s.starts_with("<command-") || s.starts_with("<local-command") || s.starts_with("<system-reminder") || s.starts_with("<agent-message") {
        acc.tocca(ts);
        return;
    }
    acc.prompt(corto(s, 300), ts);
}
