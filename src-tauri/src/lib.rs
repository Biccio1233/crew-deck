//! Crew Deck: la plancia dell'equipaggio degli agenti AI.
pub mod adapters;
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
        .invoke_handler(tauri::generate_handler![sessioni])
        .run(tauri::generate_context!())
        .expect("errore all'avvio di Crew Deck");
}
