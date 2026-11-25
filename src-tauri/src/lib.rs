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

const ATTESA: &str = include_str!("../../giudice/attesa.py");

/// L'hook d'attesa (`giudice/attesa.py`) è già nelle impostazioni di Claude Code? None se Claude Code non c'è.
#[tauri::command]
fn hook_attese() -> Option<bool> {
    let home = dirs::home_dir()?;
    if !home.join(".claude").is_dir() {
        return None;
    }
    let cfg = std::fs::read_to_string(home.join(".claude/settings.json")).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or(serde_json::json!({}));
    Some(VOCI_ATTESA.iter().all(|(evento, matcher, motivo)| ha_voce(&cfg, evento, matcher, motivo)))
}

/// Le voci dell'hook d'attesa: evento di Claude Code, matcher, motivo passato ad attesa.py.
/// «fine» (Stop) cancella il segnale quando Claude ha finito di rispondere.
const VOCI_ATTESA: [(&str, &str, &str); 3] = [("PreToolUse", "AskUserQuestion", "domanda"), ("Notification", "permission_prompt", "permesso"), ("Stop", "", "fine")];

/// Il comando lancia attesa.py con questo motivo? Percorso con o senza virgolette, semplici o doppie.
fn comando_attesa(comando: &str, motivo: &str) -> bool {
    let parole: Vec<_> = comando.split_whitespace().map(|w| w.trim_matches(['"', '\''])).collect();
    parole.windows(2).any(|w| (w[0] == "attesa.py" || w[0].ends_with("/attesa.py")) && w[1] == motivo)
}

/// C'è già, sotto `evento` e con lo stesso matcher, un comando di attesa.py con questo motivo?
/// Per Stop il matcher non conta (Claude Code lo ignora).
fn ha_voce(cfg: &serde_json::Value, evento: &str, matcher: &str, motivo: &str) -> bool {
    cfg["hooks"][evento].as_array().is_some_and(|l| {
        l.iter().any(|v| {
            (matcher.is_empty() || v["matcher"].as_str() == Some(matcher))
                && v["hooks"].as_array().is_some_and(|h| h.iter().any(|c| c["command"].as_str().is_some_and(|t| comando_attesa(t, motivo))))
        })
    })
}

/// Su richiesta: copia l'hook in ~/.crew-deck/attesa.py e lo aggiunge a ~/.claude/settings.json, dopo una copia di sicurezza.
#[tauri::command]
fn attiva_hook_attese() -> Result<String, String> {
    let home = dirs::home_dir().ok_or("cartella home non trovata")?;
    let script = home.join(".crew-deck/attesa.py");
    // il percorso finisce tra virgolette in un comando di shell: niente caratteri che la shell interpreterebbe
    if script.to_string_lossy().contains(['"', '$', '`', '\\']) {
        return Err("il percorso della cartella home contiene caratteri speciali: attivazione automatica non possibile".into());
    }
    std::fs::create_dir_all(home.join(".crew-deck")).map_err(|e| e.to_string())?;
    std::fs::write(&script, ATTESA).map_err(|e| e.to_string())?;
    let file = home.join(".claude/settings.json");
    let testo = std::fs::read_to_string(&file).unwrap_or_else(|_| "{}".into());
    let mut cfg: serde_json::Value = serde_json::from_str(&testo).map_err(|_| "settings.json di Claude Code non è leggibile: non lo tocco")?;
    let mancanti: Vec<_> = VOCI_ATTESA.iter().filter(|(evento, matcher, motivo)| !ha_voce(&cfg, evento, matcher, motivo)).collect();
    if mancanti.is_empty() {
        return Ok("Già attivo".into());
    }
    if file.exists() {
        let copia = home.join(format!(".claude/settings.json.bak-crewdeck-{}", ora_ms()));
        std::fs::copy(&file, &copia).map_err(|e| e.to_string())?;
    }
    let voce = |matcher: &str, motivo: &str| {
        serde_json::json!({"matcher": matcher, "hooks": [{"type": "command", "timeout": 5,
            "command": format!("python3 \"{}\" {motivo} 2>/dev/null || true", script.display())}]})
    };
    let hooks = cfg.as_object_mut().ok_or("settings.json non è un oggetto")?.entry("hooks").or_insert(serde_json::json!({}));
    for (evento, matcher, motivo) in mancanti {
        let lista = hooks.as_object_mut().ok_or("hooks non è un oggetto")?.entry(*evento).or_insert(serde_json::json!([]));
        lista.as_array_mut().ok_or("formato di hooks inatteso")?.push(voce(matcher, motivo));
    }
    // scrittura atomica: file accanto, poi rinomina; un'interruzione non lascia mai settings.json a metà.
    // Se settings.json è un collegamento, si sostituisce il file a cui punta; i permessi restano quelli di prima.
    let collegamento = std::fs::symlink_metadata(&file).is_ok_and(|m| m.file_type().is_symlink());
    let bersaglio = match std::fs::canonicalize(&file) {
        Ok(b) => b,
        Err(_) if collegamento => return Err("settings.json è un collegamento a un file che non esiste: non lo tocco".into()),
        Err(_) => file.clone(),
    };
    let cartella = bersaglio.parent().ok_or("cartella di settings.json non trovata")?;
    let temp = cartella.join(format!(".settings.json.tmp-crewdeck-{}", ora_ms()));
    let scrivi = || -> std::io::Result<()> {
        use std::os::unix::fs::OpenOptionsExt;
        let mut f = std::fs::OpenOptions::new().write(true).create_new(true).mode(0o600).open(&temp)?;
        std::io::Write::write_all(&mut f, (serde_json::to_string_pretty(&cfg).unwrap() + "\n").as_bytes())?;
        if let Ok(m) = std::fs::metadata(&bersaglio) {
            std::fs::set_permissions(&temp, m.permissions())?;
        }
        std::fs::rename(&temp, &bersaglio)
    };
    scrivi().map_err(|e| { let _ = std::fs::remove_file(&temp); e.to_string() })?;
    Ok("Attivato: vale per le sessioni di Claude Code aperte da adesso".into())
}

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

/// Finestra di macOS per scegliere una cartella (None se annullata).
#[tauri::command]
async fn scegli_cartella(titolo: String) -> Result<Option<String>, String> {
    tauri::async_runtime::spawn_blocking(move || carteggio::scegli_cartella(&titolo)).await.map_err(|e| e.to_string())?
}

/// Riga del cervello centrale da mettere in testa al primo messaggio.
#[tauri::command]
fn riga_cervello(percorso: String) -> String {
    carteggio::riga_cervello(&percorso)
}

/// «Creane uno»: apre una sessione nuova nella cartella con la procedura guidata.
#[tauri::command]
async fn crea_cervello(agente: String, cartella: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || carteggio::nuova_sessione(&agente, &cartella, &carteggio::richiesta_crea_cervello())).await.map_err(|e| e.to_string())?
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
        .invoke_handler(tauri::generate_handler![sessioni, apri_sessione, giudica_prompt, riscrivi_prompt, suggerisci_risposta, invia_prompt, nuova_sessione, scegli_cartella, riga_cervello, crea_cervello, chiudi_sessione, stato_cmux, debriefing, hook_attese, attiva_hook_attese])
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

#[cfg(test)]
mod prova_hook {
    use super::*;

    #[test]
    fn riconosce_i_comandi_scritti_a_mano() {
        assert!(comando_attesa("python3 \"/a b/attesa.py\" domanda 2>/dev/null || true", "domanda"));
        assert!(comando_attesa("python3 /x/giudice/attesa.py permesso", "permesso"));
        assert!(comando_attesa("python3 '/x/attesa.py' fine", "fine"));
        assert!(!comando_attesa("python3 /x/attesa.py domanda", "permesso"));
        assert!(!comando_attesa("python3 /x/attesa.py domandaX", "domanda"));
        assert!(!comando_attesa("python3 /hooks/custom_attesa.py domanda", "domanda"));
    }

    #[test]
    fn attiva_hook_attese_su_home_finta() {
        let home = std::env::temp_dir().join(format!("crewdeck-hook-{}", ora_ms()));
        std::fs::create_dir_all(home.join(".claude")).unwrap();
        // unico test che cambia HOME, nel suo modulo
        unsafe { std::env::set_var("HOME", &home) };
        let file = home.join(".claude/settings.json");
        std::fs::write(&file, r#"{"model":"x","hooks":{"PreToolUse":[{"matcher":"Bash","hooks":[]}]}}"#).unwrap();
        assert_eq!(hook_attese(), Some(false));
        attiva_hook_attese().unwrap();
        let cfg: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
        assert_eq!(cfg["model"], "x");
        assert_eq!(cfg["hooks"]["PreToolUse"].as_array().unwrap().len(), 2);
        assert_eq!(cfg["hooks"]["Notification"][0]["matcher"], "permission_prompt");
        assert!(home.join(".crew-deck/attesa.py").exists());
        let copie = std::fs::read_dir(home.join(".claude")).unwrap().filter(|e| e.as_ref().unwrap().file_name().to_string_lossy().contains("bak-crewdeck")).count();
        assert_eq!(copie, 1);
        assert_eq!(cfg["hooks"]["Stop"][0]["hooks"][0]["command"].as_str().unwrap().contains("attesa.py\" fine"), true);
        assert_eq!(hook_attese(), Some(true));
        assert_eq!(attiva_hook_attese().unwrap(), "Già attivo");
        // nessun file temporaneo lasciato accanto alle impostazioni
        assert!(!std::fs::read_dir(home.join(".claude")).unwrap().any(|e| e.unwrap().file_name().to_string_lossy().contains("tmp")));
        // installazione di prima (senza la voce «fine»): si aggiunge solo quella
        std::fs::write(&file, r#"{"hooks":{"PreToolUse":[{"matcher":"AskUserQuestion","hooks":[{"type":"command","command":"python3 \"/x/attesa.py\" domanda"}]}],"Notification":[{"matcher":"permission_prompt","hooks":[{"type":"command","command":"python3 \"/x/attesa.py\" permesso"}]}]}}"#).unwrap();
        assert_eq!(hook_attese(), Some(false));
        attiva_hook_attese().unwrap();
        let cfg: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
        assert_eq!(cfg["hooks"]["PreToolUse"].as_array().unwrap().len(), 1);
        assert_eq!(cfg["hooks"]["Notification"].as_array().unwrap().len(), 1);
        assert_eq!(cfg["hooks"]["Stop"].as_array().unwrap().len(), 1);
        assert_eq!(hook_attese(), Some(true));
        // voce con il matcher sbagliato: non conta, si aggiunge quella giusta
        std::fs::write(&file, r#"{"hooks":{"PreToolUse":[{"matcher":"Bash","hooks":[{"type":"command","command":"python3 \"/x/attesa.py\" domanda"}]}]}}"#).unwrap();
        assert_eq!(hook_attese(), Some(false));
        attiva_hook_attese().unwrap();
        let cfg: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
        assert_eq!(cfg["hooks"]["PreToolUse"][1]["matcher"], "AskUserQuestion");
        // collegamento a un altro file con permessi 0600: resta un collegamento, il bersaglio tiene i permessi
        use std::os::unix::fs::PermissionsExt;
        let vero = home.join("dotfiles-settings.json");
        std::fs::write(&vero, "{}").unwrap();
        std::fs::set_permissions(&vero, std::fs::Permissions::from_mode(0o600)).unwrap();
        std::fs::remove_file(&file).unwrap();
        std::os::unix::fs::symlink(&vero, &file).unwrap();
        attiva_hook_attese().unwrap();
        assert!(std::fs::symlink_metadata(&file).unwrap().file_type().is_symlink());
        assert!(std::fs::read_to_string(&vero).unwrap().contains("attesa.py"));
        assert_eq!(std::fs::metadata(&vero).unwrap().permissions().mode() & 0o777, 0o600);
        std::fs::remove_file(&file).unwrap();
        // collegamento a un file che non esiste: si rifiuta e il collegamento resta
        std::os::unix::fs::symlink(home.join("non-esiste/settings.json"), &file).unwrap();
        assert!(attiva_hook_attese().is_err());
        assert!(std::fs::symlink_metadata(&file).unwrap().file_type().is_symlink());
        std::fs::remove_file(&file).unwrap();
        // file illeggibile: non si tocca
        std::fs::write(&file, "{ rotto").unwrap();
        assert!(attiva_hook_attese().is_err());
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "{ rotto");
        std::fs::remove_dir_all(&home).ok();
    }
}
