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
async fn stato_cmux() -> serde_json::Value {
    tauri::async_runtime::spawn_blocking(azioni::stato_cmux).await.unwrap_or_default()
}

#[tauri::command]
fn debriefing(giorni: Option<i64>) -> serde_json::Value {
    carteggio::debriefing(giorni.unwrap_or(7))
}

/// Le app aperte dal Finder ricevono un PATH ridotto (/usr/bin:/bin…): senza questo non trovano
/// cmux, claude, codex installati con Homebrew o in ~/.local/bin. Si prende il PATH della shell di
/// login dell'utente e si aggiungono le cartelle comuni.
pub fn completa_path() {
    let home = dirs::home_dir().unwrap_or_default();
    let mut parti: Vec<String> = std::env::var("PATH").unwrap_or_default().split(':').filter(|s| !s.is_empty()).map(String::from).collect();
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".into());
    if let Ok(out) = std::process::Command::new(&shell).args(["-lc", "printf %s \"$PATH\""]).output() {
        for p in String::from_utf8_lossy(&out.stdout).split(':') {
            if !p.is_empty() && !parti.iter().any(|x| x == p) { parti.push(p.to_string()); }
        }
    }
    for p in ["/opt/homebrew/bin", "/usr/local/bin", &format!("{}/.local/bin", home.display()), &format!("{}/.cargo/bin", home.display()), "/Applications/cmux.app/Contents/Resources/bin"] {
        if !parti.iter().any(|x| x == p) { parti.push(p.to_string()); }
    }
    // SAFETY: chiamata all'avvio, prima che l'app crei altri thread che leggono l'ambiente.
    unsafe { std::env::set_var("PATH", parti.join(":")) };
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    completa_path();
    let home = dirs::home_dir().expect("cartella home non trovata");
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(Plancia(Mutex::new(Scanner::new(&home))))
        .menu(menu_italiano)
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
        .invoke_handler(tauri::generate_handler![sessioni, apri_sessione, giudica_prompt, riscrivi_prompt, suggerisci_risposta, invia_prompt, nuova_sessione, chiudi_sessione, stato_cmux, debriefing])
        .run(tauri::generate_context!())
        .expect("errore all'avvio di Crew Deck");
}

/// Menu dell'app in italiano: quello predefinito di Tauri è in inglese. Modifica serve anche a ⌘C/⌘V nei campi.
fn menu_italiano<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> tauri::Result<tauri::menu::Menu<R>> {
    use tauri::menu::{AboutMetadata, Menu, PredefinedMenuItem as P, Submenu};
    let sep = || P::separator(app);
    let app_menu = Submenu::with_items(app, "Crew Deck", true, &[
        &P::about(app, Some("Informazioni su Crew Deck"), Some(AboutMetadata::default()))?,
        &sep()?,
        &P::services(app, Some("Servizi"))?,
        &sep()?,
        &P::hide(app, Some("Nascondi Crew Deck"))?,
        &P::hide_others(app, Some("Nascondi altre"))?,
        &P::show_all(app, Some("Mostra tutte"))?,
        &sep()?,
        &P::quit(app, Some("Esci da Crew Deck"))?,
    ])?;
    let modifica = Submenu::with_items(app, "Modifica", true, &[
        &P::undo(app, Some("Annulla"))?,
        &P::redo(app, Some("Ripeti"))?,
        &sep()?,
        &P::cut(app, Some("Taglia"))?,
        &P::copy(app, Some("Copia"))?,
        &P::paste(app, Some("Incolla"))?,
        &P::select_all(app, Some("Seleziona tutto"))?,
    ])?;
    let vista = Submenu::with_items(app, "Vista", true, &[&P::fullscreen(app, Some("Schermo intero"))?])?;
    let finestra = Submenu::with_items(app, "Finestra", true, &[
        &P::minimize(app, Some("Riduci nel Dock"))?,
        &P::maximize(app, Some("Ingrandisci"))?,
        &sep()?,
        &P::close_window(app, Some("Chiudi finestra"))?,
    ])?;
    Menu::with_items(app, &[&app_menu, &modifica, &vista, &finestra])
}
