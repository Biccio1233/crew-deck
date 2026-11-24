//! Un test per ogni regola del motore di stato, sui tre formati di registro.
use crew_deck_lib::adapters::feed;
use crew_deck_lib::adapters::codex::cmd_da_script;
use crew_deck_lib::engine::{calcola, ruolo, Motivo, Opzioni, Ruolo, Stato};
use crew_deck_lib::model::{Acc, Agente};
use serde_json::{json, Value};
use std::path::PathBuf;

const T0: i64 = 1_800_000_000_000; // un istante fisso

fn ts(sec: i64) -> String {
    chrono::DateTime::from_timestamp_millis(T0 + sec * 1000).unwrap().to_rfc3339()
}

fn acc(agente: Agente, righe: &[Value]) -> Acc {
    let mut a = Acc::new(agente, "s1".into(), PathBuf::from("/tmp/s1.jsonl"));
    for r in righe {
        feed(&mut a, r);
    }
    a
}

fn stato_a(a: &Acc, sec: i64) -> (Stato, Motivo, bool) {
    let c = calcola(a, T0 + sec * 1000, &Opzioni::default());
    (c.stato, c.motivo, c.stimato)
}

// ---------- Claude ----------
fn c_user(sec: i64, testo: &str) -> Value {
    json!({"type":"user","timestamp":ts(sec),"cwd":"/p/progetto","entrypoint":"cli","message":{"role":"user","content":testo}})
}
fn c_tool(sec: i64, id: &str, nome: &str, input: Value) -> Value {
    json!({"type":"assistant","timestamp":ts(sec),"message":{"id":format!("m{sec}"),"model":"claude-x","stop_reason":"tool_use",
        "content":[{"type":"tool_use","id":id,"name":nome,"input":input}],
        "usage":{"input_tokens":10,"output_tokens":5,"cache_read_input_tokens":100,"cache_creation_input_tokens":0}}})
}
fn c_result(sec: i64, id: &str) -> Value {
    json!({"type":"user","timestamp":ts(sec),"message":{"role":"user","content":[{"type":"tool_result","tool_use_id":id,"content":"ok"}]}})
}
fn c_fine(sec: i64) -> Value {
    json!({"type":"assistant","timestamp":ts(sec),"message":{"id":format!("m{sec}"),"stop_reason":"end_turn","content":[{"type":"text","text":"fatto"}]}})
}

#[test]
fn claude_strumento_lento_lavora() {
    let a = acc(Agente::Claude, &[c_user(0, "lancia i test"), c_tool(1, "t1", "Bash", json!({"command":"npm test","description":"Avvia i test"}))]);
    assert_eq!(stato_a(&a, 120), (Stato::Lavora, Motivo::Strumento, false));
    assert_eq!(calcola(&a, T0 + 120_000, &Opzioni::default()).azione, "Esegue: Avvia i test");
    assert_eq!(a.cwd.as_deref(), Some("/p/progetto"));
}

#[test]
fn claude_strumento_veloce_fermo_chiede_permesso_stimato() {
    let a = acc(Agente::Claude, &[c_user(0, "modifica"), c_tool(1, "t1", "Edit", json!({"file_path":"/p/app.ts"}))]);
    assert_eq!(stato_a(&a, 5).0, Stato::Lavora);
    assert_eq!(stato_a(&a, 20), (Stato::Aspetta, Motivo::PermessoStimato, true));
}

#[test]
fn claude_domanda_aspetta_te() {
    let a = acc(Agente::Claude, &[c_user(0, "scegli"), c_tool(1, "q1", "AskUserQuestion", json!({}))]);
    assert_eq!(stato_a(&a, 600), (Stato::Aspetta, Motivo::Domanda, false));
}

#[test]
fn claude_fine_turno_e_dedup_usage() {
    let a = acc(Agente::Claude, &[c_user(0, "ciao"), c_tool(1, "t1", "Read", json!({"file_path":"/p/a.md"})), c_tool(1, "t2", "Read", json!({})), c_result(2, "t1"), c_result(2, "t2"), c_fine(3)]);
    assert_eq!(stato_a(&a, 10), (Stato::Finito, Motivo::FineTurno, false));
    // stesso id di messaggio (m1) ripetuto: l'usage si conta una volta
    assert_eq!(a.token.input, 10);
    assert_eq!(a.n_prompt, 1);
}

#[test]
fn claude_interrotto() {
    let a = acc(Agente::Claude, &[c_user(0, "vai"), c_tool(1, "t1", "Bash", json!({})), c_user(5, "[Request interrupted by user]")]);
    assert_eq!(stato_a(&a, 10), (Stato::Finito, Motivo::Interrotto, false));
}

#[test]
fn claude_silenzio_lungo_in_porto() {
    let a = acc(Agente::Claude, &[c_user(0, "vai"), c_tool(1, "t1", "Bash", json!({}))]);
    assert_eq!(stato_a(&a, 31 * 60).0, Stato::Inattivo);
}

#[test]
fn claude_sessione_sdk_e_automatica() {
    let r = json!({"type":"user","timestamp":ts(0),"entrypoint":"sdk-cli","message":{"content":"estrai"}});
    let a = acc(Agente::Claude, &[r]);
    assert!(a.automatica);
}

#[test]
fn ruolo_dagli_strumenti() {
    let a = acc(Agente::Claude, &[c_user(0, "x"), c_tool(1, "a", "Edit", json!({})), c_tool(2, "b", "Edit", json!({})), c_tool(3, "c", "Read", json!({}))]);
    assert_eq!(ruolo(&a), Ruolo::Grinder);
    let b = acc(Agente::Claude, &[c_user(0, "x"), c_tool(1, "a", "Grep", json!({})), c_tool(2, "b", "Read", json!({}))]);
    assert_eq!(ruolo(&b), Ruolo::Prodiere);
}

// ---------- Codex ----------
fn x(sec: i64, tipo: &str, payload: Value) -> Value {
    json!({"timestamp":ts(sec),"type":tipo,"payload":payload})
}

#[test]
fn codex_ciclo_completo() {
    let righe = [
        x(0, "session_meta", json!({"id":"abc","cwd":"/p/codex","originator":"Codex Desktop"})),
        x(1, "event_msg", json!({"type":"task_started"})),
        x(1, "event_msg", json!({"type":"user_message","message":"sistema il bug"})),
        x(2, "response_item", json!({"type":"custom_tool_call","call_id":"c1","name":"exec","input":"text(await tools.exec_command({cmd:\"cargo test\"}))"})),
    ];
    let a = acc(Agente::Codex, &righe);
    assert_eq!(a.id, "abc");
    assert_eq!(stato_a(&a, 30).0, Stato::Lavora); // exec è lento: niente permesso stimato
    assert_eq!(calcola(&a, T0 + 30_000, &Opzioni::default()).azione, "Esegue: cargo test");
    let mut righe2 = righe.to_vec();
    righe2.push(x(40, "response_item", json!({"type":"custom_tool_call_output","call_id":"c1"})));
    righe2.push(x(41, "event_msg", json!({"type":"token_count","info":{"total_token_usage":{"input_tokens":500,"output_tokens":20,"cached_input_tokens":300}}})));
    righe2.push(x(42, "event_msg", json!({"type":"task_complete"})));
    let b = acc(Agente::Codex, &righe2);
    assert_eq!(stato_a(&b, 50), (Stato::Finito, Motivo::FineTurno, false));
    assert_eq!(b.token.input, 500);
    assert_eq!(b.ultimo_prompt.as_deref(), Some("sistema il bug"));
}

#[test]
fn codex_cmd_da_script() {
    assert_eq!(cmd_da_script(r#"text(await tools.exec_command({cmd:"rg -n \"x\" a.md",max:3}))"#).as_deref(), Some(r#"rg -n "x" a.md"#));
    assert_eq!(cmd_da_script("text(1)"), None);
}

// ---------- pi ----------
#[test]
fn pi_ciclo_con_costo() {
    let righe = [
        json!({"type":"session","id":"pi1","timestamp":ts(0),"cwd":"/p/pi"}),
        json!({"type":"message","timestamp":ts(1),"message":{"role":"user","content":[{"type":"text","text":"elenca i file"}]}}),
        json!({"type":"message","timestamp":ts(2),"message":{"role":"assistant","model":"m","stopReason":"toolUse",
            "content":[{"type":"toolCall","id":"k1","name":"bash","arguments":{"command":"ls"}}],
            "usage":{"input":3,"output":4,"cacheRead":5,"cacheWrite":6,"cost":{"total":0.25}}}}),
    ];
    let a = acc(Agente::Pi, &righe);
    assert_eq!(stato_a(&a, 5).0, Stato::Lavora);
    let mut r2 = righe.to_vec();
    r2.push(json!({"type":"message","timestamp":ts(3),"message":{"role":"toolResult","toolCallId":"k1","isError":false}}));
    r2.push(json!({"type":"message","timestamp":ts(4),"message":{"role":"assistant","stopReason":"stop","content":[{"type":"text","text":"ecco"}],"usage":{"input":1,"output":1,"cost":{"total":0.5}}}}));
    let b = acc(Agente::Pi, &r2);
    assert_eq!(stato_a(&b, 10), (Stato::Finito, Motivo::FineTurno, false));
    assert!((b.costo_usd.unwrap() - 0.75).abs() < 1e-9);
    assert_eq!(b.cwd.as_deref(), Some("/p/pi"));
}

#[test]
fn nomi_leggibili_di_strumenti_e_cartelle() {
    use crew_deck_lib::model::azione_strumento;
    assert_eq!(azione_strumento("mcp__cmux-cua__get_window_state", &json!({})), "Usa cmux-cua: get window state");
    assert_eq!(azione_strumento("mcp__plugin_firecrawl_firecrawl__scrape", &json!({})), "Usa firecrawl firecrawl: scrape");
    assert_eq!(azione_strumento("Strano", &json!({})), "Usa Strano");
    let home = dirs::home_dir().unwrap().to_string_lossy().to_string();
    assert_eq!(crew_deck_lib::scanner::progetto(&home), "Cartella home");
    assert_eq!(crew_deck_lib::scanner::progetto(&format!("{home}/")), "Cartella home");
    assert_eq!(crew_deck_lib::scanner::progetto("/tmp/esempio/checkout-api"), "checkout-api");
}
