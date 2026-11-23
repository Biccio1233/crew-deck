//! Crew Deck: la plancia dell'equipaggio degli agenti AI.
pub mod adapters;
pub mod azioni;
pub mod carteggio;
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
    let (pos, file, cwd, ultima_app, origine) = {
        let mut s = plancia.0.lock().unwrap();
        s.localizza_ora();
        s.per_azione(ag, &id).ok_or("sessione non trovata")?
    };
    // sessioni nate nelle app desktop: si aprono nell'app, sulla conversazione giusta
    if let Some((url, nome)) = azioni::link_desktop(ag, &id, origine.as_deref()) {
        return azioni::apri_link(&url, nome);
    }
    if let Some(p) = pos {
        return azioni::vai(&p);
    }
    let cwd = cwd.ok_or("cartella della sessione sconosciuta")?;
    let term = terminale.or(ultima_app).filter(|t| t == "cmux" || t == "Terminal").unwrap_or_else(|| "Ghostty".into());
    azioni::riprendi(ag, &id, &file, &cwd, &term)
}

fn contesto_di(plancia: &tauri::State<Plancia>, agente: &Option<String>, id: &Option<String>) -> (Option<localizza::Posizione>, Option<String>) {
    match (agente, id) {
        (Some(a), Some(i)) => match agente_da(a) {
            Ok(ag) => {
                let mut s = plancia.0.lock().unwrap();
                s.localizza_ora();
                s.contesto(ag, i).map(|(p, t, _)| (p, t)).unwrap_or((None, None))
            }
            Err(_) => (None, None),
        },
        _ => (None, None),
    }
}

/// Giudizio di Jev sul prompt, con l'ultimo messaggio dell'agente della sessione scelta come contesto.
#[tauri::command]
async fn giudica_prompt(plancia: tauri::State<'_, Plancia>, testo: String, agente: Option<String>, id: Option<String>) -> Result<serde_json::Value, String> {
    let (_, prima) = contesto_di(&plancia, &agente, &id);
    tauri::async_runtime::spawn_blocking(move || carteggio::giudica(&testo, prima.as_deref())).await.map_err(|e| e.to_string())?
}

/// Rifinisci o Approfondisci con Claude.
#[tauri::command]
async fn riscrivi_prompt(plancia: tauri::State<'_, Plancia>, modo: String, testo: String, agente: Option<String>, id: Option<String>, mancanti: Vec<String>) -> Result<serde_json::Value, String> {
    let (_, prima) = contesto_di(&plancia, &agente, &id);
    tauri::async_runtime::spawn_blocking(move || carteggio::riscrivi(&modo, &testo, prima.as_deref(), &mancanti)).await.map_err(|e| e.to_string())?
}

/// Scrive il prompt nella scheda della sessione (solo dopo la conferma nell'interfaccia).
#[tauri::command]
async fn invia_prompt(plancia: tauri::State<'_, Plancia>, agente: String, id: String, testo: String, invio: bool) -> Result<String, String> {
    let (pos, _) = contesto_di(&plancia, &Some(agente), &Some(id));
    let pos = pos.ok_or("la sessione non è aperta in nessun terminale: usa «Nuova sessione»")?;
    tauri::async_runtime::spawn_blocking(move || carteggio::invia(&pos, &testo, invio)).await.map_err(|e| e.to_string())?
}

/// Suggerisce cosa rispondere alla sessione scelta.
#[tauri::command]
async fn suggerisci_risposta(plancia: tauri::State<'_, Plancia>, agente: String, id: String) -> Result<serde_json::Value, String> {
    let ag = agente_da(&agente)?;
    let (richiesta, risposta) = {
        let s = plancia.0.lock().unwrap();
        s.conversazione(ag, &id).ok_or("sessione non trovata")?
    };
    let risposta = risposta.ok_or("l'agente non ha ancora risposto")?;
    tauri::async_runtime::spawn_blocking(move || carteggio::suggerisci(richiesta.as_deref(), &risposta)).await.map_err(|e| e.to_string())?
}

#[tauri::command]
async fn nuova_sessione(agente: String, cartella: String, testo: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || carteggio::nuova_sessione(&agente, &cartella, &testo)).await.map_err(|e| e.to_string())?
}

/// Chiude la sessione nella vista (o la ripristina). Non tocca i file della sessione.
#[tauri::command]
fn chiudi_sessione(plancia: tauri::State<Plancia>, agente: String, id: String, ripristina: Option<bool>) -> Result<(), String> {
    let ag = agente_da(&agente)?;
    let mut s = plancia.0.lock().unwrap();
    if ripristina.unwrap_or(false) {
        s.ripristina(ag, &id);
        Ok(())
    } else if s.chiudi(ag, &id) {
        Ok(())
    } else {
        Err("sessione non trovata".into())
    }
}

#[tauri::command]
fn debriefing(giorni: Option<i64>) -> serde_json::Value {
    carteggio::debriefing(giorni.unwrap_or(7))
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
        .invoke_handler(tauri::generate_handler![sessioni, apri_sessione, giudica_prompt, riscrivi_prompt, suggerisci_risposta, invia_prompt, nuova_sessione, chiudi_sessione, debriefing])
        .run(tauri::generate_context!())
        .expect("errore all'avvio di Crew Deck");
}
