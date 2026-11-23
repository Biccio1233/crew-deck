//! Crew Deck: la plancia dell'equipaggio degli agenti AI.
pub mod adapters;
pub mod azioni;
pub mod localizza;
pub mod engine;
pub mod model;
pub mod scanner;

use scanner::{Scanner, Vista};
use std::sync::Mutex;
use std::time::Duration;
use tauri::{Emitter, Manager};

pub fn ora_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

pub struct Plancia(pub Mutex<Scanner>);

/// Elenco delle sessioni con lo stato calcolato adesso.
#[tauri::command]
fn sessioni(plancia: tauri::State<Plancia>) -> Vec<Vista> {
    let mut s = plancia.0.lock().unwrap();
    s.aggiorna();
    s.viste(ora_ms())
}

fn agente_da(s: &str) -> Result<model::Agente, String> {
    match s {
        "claude" => Ok(model::Agente::Claude),
        "codex" => Ok(model::Agente::Codex),
        "pi" => Ok(model::Agente::Pi),
        _ => Err(format!("agente sconosciuto: {s}")),
    }
}

/// Porta in primo piano la scheda dove gira la sessione; se è chiusa, la riprende.
#[tauri::command]
fn apri_sessione(plancia: tauri::State<Plancia>, agente: String, id: String, terminale: Option<String>) -> Result<String, String> {
    let ag = agente_da(&agente)?;
    let (pos, file, cwd, ultima_app) = {
        let mut s = plancia.0.lock().unwrap();
        s.localizza_ora();
        s.per_azione(ag, &id).ok_or("sessione non trovata")?
    };
    if let Some(p) = pos {
        return azioni::vai(&p);
    }
    let cwd = cwd.ok_or("cartella della sessione sconosciuta")?;
    let term = terminale.or(ultima_app).filter(|t| t == "cmux" || t == "Terminal").unwrap_or_else(|| "Ghostty".into());
    azioni::riprendi(ag, &id, &file, &cwd, &term)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let home = dirs::home_dir().expect("cartella home non trovata");
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(Plancia(Mutex::new(Scanner::new(&home))))
        .setup(|app| {
            // vedetta: ogni 2 secondi rilegge le code dei registri e manda lo stato all'interfaccia
            let handle = app.handle().clone();
            std::thread::spawn(move || loop {
                let viste = {
                    let p = handle.state::<Plancia>();
                    let mut s = p.0.lock().unwrap();
                    s.aggiorna();
                    s.viste(ora_ms())
                };
                let _ = handle.emit("sessioni", &viste);
                std::thread::sleep(Duration::from_secs(2));
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![sessioni, apri_sessione])
        .run(tauri::generate_context!())
        .expect("errore all'avvio di Crew Deck");
}
