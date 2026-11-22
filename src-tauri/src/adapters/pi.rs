//! pi: `~/.pi/agent/sessions/<cartella>/<data>_<id>.jsonl`.
use crate::model::{azione_strumento, corto, ts_ms, Acc, Ev, Token};
use serde_json::Value;

pub fn feed(acc: &mut Acc, riga: &Value) {
    let tipo = riga.get("type").and_then(|v| v.as_str()).unwrap_or("");
    let Some(ts) = ts_ms(riga.get("timestamp")) else { return };
    match tipo {
        "session" => {
            if let Some(id) = riga.get("id").and_then(|v| v.as_str()) {
                acc.id = id.to_string();
            }
            if let Some(c) = riga.get("cwd").and_then(|v| v.as_str()) {
                acc.cwd = Some(c.to_string());
            }
            acc.tocca(ts);
        }
        "model_change" => {
            if let Some(m) = riga.get("modelId").and_then(|v| v.as_str()) {
                acc.modello = Some(m.to_string());
            }
            acc.tocca(ts);
        }
        "usage" => {
            costo(acc, riga.get("usage"));
            acc.tocca(ts);
        }
        "message" => {
            let m = riga.get("message").cloned().unwrap_or(Value::Null);
            match m.get("role").and_then(|v| v.as_str()) {
                Some("user") => {
                    let t = testo(&m);
                    if !t.trim().is_empty() {
                        acc.prompt(corto(&t, 300), ts);
                        if acc.titolo.is_none() {
                            acc.titolo = Some(corto(&t, 60));
                        }
                    }
                }
                Some("assistant") => {
                    if let Some(mod_) = m.get("model").and_then(|v| v.as_str()) {
                        acc.modello = Some(mod_.to_string());
                    }
                    if let Some(u) = m.get("usage") {
                        acc.usage(
                            None,
                            Token {
                                input: u.get("input").and_then(|v| v.as_u64()).unwrap_or(0),
                                output: u.get("output").and_then(|v| v.as_u64()).unwrap_or(0),
                                cache_lettura: u.get("cacheRead").and_then(|v| v.as_u64()).unwrap_or(0),
                                cache_scrittura: u.get("cacheWrite").and_then(|v| v.as_u64()).unwrap_or(0),
                            },
                        );
                        costo(acc, Some(u));
                    }
                    let mut ultimo = Ev::Testo;
                    if let Some(Value::Array(parti)) = m.get("content") {
                        for p in parti {
                            match p.get("type").and_then(|v| v.as_str()) {
                                Some("toolCall") => {
                                    let id = p.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
                                    let nome = p.get("name").and_then(|v| v.as_str()).unwrap_or("?").to_string();
                                    let args = p.get("arguments").cloned().unwrap_or(Value::Null);
                                    let azione = azione_strumento(&nome, &args);
                                    acc.apri(id, nome, azione, ts);
                                    ultimo = Ev::Strumento;
                                }
                                Some("thinking") => ultimo = Ev::Pensa,
                                Some("text") => ultimo = Ev::Testo,
                                _ => {}
                            }
                        }
                    }
                    match m.get("stopReason").and_then(|v| v.as_str()) {
                        Some("stop") | Some("length") => acc.fine_turno(ts),
                        Some("aborted") => {
                            acc.aperti.clear();
                            acc.evento(Ev::Interrotto, ts);
                        }
                        Some("error") => {
                            acc.errori += 1;
                            acc.evento(Ev::ErroreApi, ts);
                        }
                        _ => {
                            if ultimo != Ev::Strumento {
                                acc.evento(ultimo, ts);
                            }
                        }
                    }
                }
                Some("toolResult") => {
                    let id = m.get("toolCallId").and_then(|v| v.as_str()).unwrap_or("");
                    let err = m.get("isError").and_then(|v| v.as_bool()).unwrap_or(false);
                    acc.chiudi(id, err, ts);
                }
                _ => acc.tocca(ts),
            }
        }
        _ => acc.tocca(ts),
    }
}

fn testo(m: &Value) -> String {
    match m.get("content") {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Array(parti)) => parti
            .iter()
            .filter(|p| p.get("type").and_then(|v| v.as_str()) == Some("text"))
            .filter_map(|p| p.get("text").and_then(|v| v.as_str()))
            .collect::<Vec<_>>()
            .join(" "),
        _ => String::new(),
    }
}

fn costo(acc: &mut Acc, u: Option<&Value>) {
    if let Some(c) = u.and_then(|u| u.pointer("/cost/total")).and_then(|v| v.as_f64()) {
        acc.costo_usd = Some(acc.costo_usd.unwrap_or(0.0) + c);
    }
}
