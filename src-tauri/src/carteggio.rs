//! Sala carteggio: giudizio del prompt (Jev), riscrittura (Claude), invio nella sessione, debriefing.
use crate::localizza::Posizione;
use serde_json::{json, Value};
use std::io::Write;
use std::process::{Command, Stdio};
use std::time::Duration;

/// Il giudice è uno solo: lo stesso file usato dall'hook, incorporato nell'app.
const GIUDICE: &str = include_str!("../../giudice/giudice.py");

/// Esegue un comando con un tempo massimo, restituendo lo stdout.
fn con_tempo(mut cmd: Command, stdin: Option<&str>, secondi: u64) -> Result<String, String> {
    cmd.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut figlio = cmd.spawn().map_err(|e| format!("non riesco ad avviare il comando: {e}"))?;
    if let Some(t) = stdin {
        figlio.stdin.take().unwrap().write_all(t.as_bytes()).map_err(|e| e.to_string())?;
    } else {
        drop(figlio.stdin.take());
    }
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(figlio.wait_with_output());
    });
    match rx.recv_timeout(Duration::from_secs(secondi)) {
        Ok(Ok(out)) if out.status.success() => Ok(String::from_utf8_lossy(&out.stdout).to_string()),
        Ok(Ok(out)) => Err(String::from_utf8_lossy(&out.stderr).trim().chars().take(400).collect()),
        Ok(Err(e)) => Err(e.to_string()),
        Err(_) => Err(format!("nessuna risposta entro {secondi} secondi")),
    }
}

/// Giudizio di Jev sul prompt. Restituisce il JSON del giudice ({tipo, punti…} o {saltato}/{errore}).
pub fn giudica(testo: &str, prima: Option<&str>) -> Result<Value, String> {
    let mut cmd = Command::new("python3");
    cmd.arg("-").arg("--json").arg(testo);
    if let Some(p) = prima {
        cmd.arg("--prima").arg(p);
    }
    let out = con_tempo(cmd, Some(GIUDICE), 30)?;
    serde_json::from_str(out.trim()).map_err(|e| format!("risposta del giudice non leggibile: {e}"))
}

const RIFINISCI: &str = r#"Sei un esperto di richieste per agenti AI di programmazione e di lavoro (Claude Code, Codex, pi).
Migliora la richiesta qui sotto con MODIFICHE MINIME: tieni lingua, tono, parole e intenzione dell'autore.
Aggiungi solo ciò che manca fra questi punti, presi dalle guide di Anthropic e OpenAI:
1. come si capisce che il lavoro è fatto: un controllo eseguibile o verificabile (test, comando, risultato atteso, esempio);
2. vincoli e cosa resta fuori;
3. perché serve, se aiuta l'agente a scegliere;
4. contesto che l'agente non può trovare da solo nei file (decisioni, preferenze, sintomi osservati);
5. su cosa lavorare, se non è chiaro.
Regole: non inventare fatti. Dove serve un'informazione che solo l'autore ha, scrivi un segnaposto [[cosa serve]] e aggiungi la domanda corrispondente. Non aggiungere formule come «pensa passo passo» o «sii accurato». Se la richiesta è già buona, cambiala il meno possibile.
Rispondi SOLO con un oggetto JSON, senza testo prima o dopo:
{"prompt": "la richiesta migliorata", "domande": ["domanda per l'autore"], "modifiche": ["cosa hai aggiunto e perché, in poche parole"]}"#;

const APPROFONDISCI: &str = r#"Voglio una versione più profonda di questo compito, non più grande. Prima di riscriverlo:
(1) cosa farebbe qui un vero professionista del settore che l'autore non sta chiedendo;
(2) da quale materiale più grezzo partirebbe, invece di quello già masticato;
(3) i metodi e gli strumenti precisi che userebbe;
(4) cosa, a parte l'agente o l'autore, dimostrerebbe che il risultato è giusto;
(5) cosa un professionista NON farebbe qui, perché resta fuori dal compito.
Poi riscrivi la richiesta chiedendo esattamente quello, nella lingua dell'autore. Non inventare fatti: dove serve un'informazione che solo l'autore ha, usa un segnaposto [[cosa serve]] e aggiungi la domanda.
Rispondi SOLO con un oggetto JSON, senza testo prima o dopo:
{"professionista": "...", "materiale": "...", "metodi": "...", "prova": "...", "fuori": "...", "domande": ["..."], "prompt": "la richiesta riscritta"}"#;

/// Riscrive il prompt con Claude (abbonamento dell'utente, niente strumenti, niente sessione salvata).
pub fn riscrivi(modo: &str, testo: &str, prima: Option<&str>, mancanti: &[String]) -> Result<Value, String> {
    let istruzioni = if modo == "approfondisci" { APPROFONDISCI } else { RIFINISCI };
    let mut richiesta = format!("{istruzioni}\n\n<richiesta>\n{testo}\n</richiesta>\n");
    if let Some(p) = prima {
        let coda: String = p.chars().rev().take(1500).collect::<String>().chars().rev().collect();
        richiesta.push_str(&format!("\n<ultimo_messaggio_dell_agente>\n{coda}\n</ultimo_messaggio_dell_agente>\n"));
    }
    if !mancanti.is_empty() {
        richiesta.push_str(&format!("\nUn controllo automatico dice che mancano: {}.\n", mancanti.join(", ")));
    }
    let mut cmd = Command::new("claude");
    cmd.args(["-p", "--tools", "", "--no-session-persistence", "--setting-sources", "", "--output-format", "text", "--model", "sonnet"])
        .env("CREWDECK_GIUDICE", "0")
        .current_dir(std::env::temp_dir());
    let out = con_tempo(cmd, Some(&richiesta), 150)?;
    estrai_json(&out).ok_or_else(|| format!("risposta non in JSON: {}", out.chars().take(200).collect::<String>()))
}

const SUGGERISCI: &str = r#"Sei l'assistente di chi dirige un agente AI (Claude Code, Codex, pi). Qui sotto ci sono l'ultima richiesta dell'utente e l'ultima risposta dell'agente.
Proponi da 2 a 3 risposte che l'utente potrebbe mandare adesso per far avanzare bene il lavoro. Regole:
- se l'agente ha fatto domande, almeno una proposta risponde a quelle domande, con segnaposti [[...]] dove serve una scelta che solo l'utente può fare;
- se l'agente dice di aver finito, una proposta chiede una verifica concreta (un test, un controllo, un esempio da vedere) prima di andare avanti;
- scrivi come scrive l'utente: italiano diretto, frasi brevi, niente formule di cortesia;
- non inventare fatti sul progetto.
Rispondi SOLO con un oggetto JSON, senza testo prima o dopo:
{"suggerimenti": [{"testo": "la risposta da mandare", "perche": "a cosa serve, in poche parole"}]}"#;

/// Propone cosa rispondere all'agente, dalla coppia ultima richiesta / ultima risposta.
pub fn suggerisci(richiesta: Option<&str>, risposta: &str) -> Result<Value, String> {
    let coda: String = risposta.chars().rev().take(2500).collect::<String>().chars().rev().collect();
    let testo = format!(
        "{SUGGERISCI}\n\n<ultima_richiesta_utente>\n{}\n</ultima_richiesta_utente>\n\n<ultima_risposta_agente>\n{coda}\n</ultima_risposta_agente>\n",
        richiesta.unwrap_or("(nessuna)")
    );
    let mut cmd = Command::new("claude");
    cmd.args(["-p", "--tools", "", "--no-session-persistence", "--setting-sources", "", "--output-format", "text", "--model", "sonnet"])
        .env("CREWDECK_GIUDICE", "0")
        .current_dir(std::env::temp_dir());
    let out = con_tempo(cmd, Some(&testo), 120)?;
    estrai_json(&out).ok_or_else(|| format!("risposta non in JSON: {}", out.chars().take(200).collect::<String>()))
}

/// Prende il primo oggetto JSON da un testo (il modello a volte aggiunge recinti ```json).
pub fn estrai_json(t: &str) -> Option<Value> {
    let i = t.find('{')?;
    let j = t.rfind('}')?;
    serde_json::from_str(&t[i..=j]).ok()
}

fn osascript(script: &str) -> Result<String, String> {
    let out = Command::new("osascript").arg("-e").arg(script).output().map_err(|e| e.to_string())?;
    if out.status.success() { Ok(String::from_utf8_lossy(&out.stdout).trim().to_string()) } else { Err(String::from_utf8_lossy(&out.stderr).trim().to_string()) }
}

fn as_str(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

fn appunti(testo: &str) -> Result<(), String> {
    con_tempo(Command::new("pbcopy"), Some(testo), 5).map(|_| ())
}

/// Scrive il prompt nella scheda della sessione. In Ghostty lo incolla e (se `invio`) preme Invio;
/// altrove lo copia negli appunti e porta la scheda in primo piano.
pub fn invia(pos: &Posizione, testo: &str, invio: bool) -> Result<String, String> {
    if pos.app == "Ghostty" {
        if let Some(tty) = &pos.tty {
            let script = format!(
                "tell application \"Ghostty\"\n repeat with w in windows\n repeat with t in tabs of w\n repeat with term in terminals of t\n if tty of term is {tty} then\n focus term\n input text {testo} to term\n {invio}\n activate\n return \"ok\"\n end if\n end repeat\n end repeat\n end repeat\n return \"non trovata\"\nend tell",
                tty = as_str(tty),
                testo = as_str(testo),
                invio = if invio { "send key \"enter\" to term" } else { "" }
            );
            if osascript(&script)? == "ok" {
                return Ok(if invio { "Inviato in Ghostty".into() } else { "Scritto in Ghostty: premi Invio quando vuoi".into() });
            }
        }
    }
    if pos.app == "cmux" {
        if let (Some(ws), Some(sf)) = (&pos.cmux_workspace, &pos.cmux_surface) {
            let mut args = vec!["paste", "--workspace", ws.as_str(), "--surface", sf.as_str()];
            if invio {
                args.push("--submit");
            }
            args.push("-");
            match crate::azioni::cmux_con(&args, Some(testo)) {
                Ok(()) => {
                    let _ = crate::azioni::vai(pos);
                    return Ok(if invio { "Inviato in cmux".into() } else { "Scritto in cmux: premi Invio quando vuoi".into() });
                }
                Err(_) => {
                    appunti(testo)?;
                    let _ = crate::azioni::vai(pos);
                    return Ok("cmux non accetta comandi da Crew Deck: il prompt è negli appunti, incollalo con ⌘V. Per l'invio diretto: cmux › Impostazioni › Automazione › modalità password.".into());
                }
            }
        }
    }
    appunti(testo)?;
    let _ = crate::azioni::vai(pos);
    Ok(format!("Copiato negli appunti: incollalo con ⌘V nella scheda di {}", pos.app))
}

/// Apre una sessione nuova in Ghostty nella cartella, con l'agente già avviato sul prompt.
pub fn nuova_sessione(agente: &str, cartella: &str, testo: &str) -> Result<String, String> {
    let file = std::env::temp_dir().join(format!("crew-deck-prompt-{}.txt", std::process::id()));
    std::fs::write(&file, testo).map_err(|e| e.to_string())?;
    let eseguibile = match agente { "codex" => "codex", "pi" => "pi", _ => "claude" };
    let comando = format!("{eseguibile} \"$(cat '{}')\"\n", file.display());
    let script = format!(
        "tell application \"Ghostty\"\n set cfg to new surface configuration\n set initial working directory of cfg to {}\n set initial input of cfg to {}\n if (count of windows) > 0 then\n new tab in front window with configuration cfg\n else\n new window with configuration cfg\n end if\n activate\nend tell",
        as_str(cartella),
        as_str(&comando)
    );
    osascript(&script).map(|_| format!("Nuova sessione di {eseguibile} aperta in Ghostty"))
}

/// Statistiche del registro dell'hook (solo numeri), degli ultimi `giorni`.
pub fn debriefing(giorni: i64) -> Value {
    let percorso = dirs::home_dir().unwrap_or_default().join(".crew-deck/prompt-registro.jsonl");
    let testo = std::fs::read_to_string(percorso).unwrap_or_default();
    let limite = chrono::Utc::now() - chrono::Duration::days(giorni);
    let mut voci = Vec::new();
    for riga in testo.lines() {
        let Ok(v) = serde_json::from_str::<Value>(riga) else { continue };
        let quando = v.get("ts").and_then(|t| t.as_str()).and_then(|t| chrono::DateTime::parse_from_str(t, "%Y-%m-%dT%H:%M:%S%z").ok());
        if quando.map(|q| q >= limite).unwrap_or(false) {
            voci.push(v);
        }
    }
    let conta = |f: &dyn Fn(&Value) -> bool| voci.iter().filter(|v| f(v)).count();
    let giudicati: Vec<&Value> = voci.iter().filter(|v| v["esito"] == "giudicato").collect();
    let compiti: Vec<&Value> = giudicati.iter().copied().filter(|v| v["tipo"] == "new_task").collect();
    let quota = |punto: &str, soglia_si: bool| -> Option<f64> {
        let validi: Vec<f64> = compiti.iter().filter_map(|v| v["punti"][punto].as_f64()).collect();
        if validi.is_empty() { return None; }
        let n = validi.iter().filter(|p| if soglia_si { **p >= 0.7 } else { **p <= 0.3 }).count();
        Some((n as f64 / validi.len() as f64 * 100.0).round())
    };
    let mut per_progetto: std::collections::BTreeMap<String, (u32, u32)> = Default::default();
    for v in &compiti {
        let p = v["progetto"].as_str().unwrap_or("?").to_string();
        let e = per_progetto.entry(p).or_default();
        e.0 += 1;
        if v["punti"]["criterio_fine"].as_f64().unwrap_or(0.0) >= 0.7 { e.1 += 1; }
    }
    let mut tipi: std::collections::BTreeMap<String, u32> = Default::default();
    for v in &giudicati {
        *tipi.entry(v["tipo"].as_str().unwrap_or("?").to_string()).or_default() += 1;
    }
    let parole: Vec<u64> = compiti.iter().filter_map(|v| v["parole"].as_u64()).collect();
    let mediana = if parole.is_empty() { None } else { let mut p = parole.clone(); p.sort(); Some(p[p.len() / 2]) };
    json!({
        "giorni": giorni,
        "totale": voci.len(),
        "giudicati": giudicati.len(),
        "saltati": { "brevi": conta(&|v| v["esito"] == "saltato_breve"), "sistema": conta(&|v| v["esito"] == "saltato_sistema"), "sensibili": conta(&|v| v["esito"] == "saltato_sensibile"), "errori": conta(&|v| v["esito"] == "errore") },
        "tipi": tipi,
        "compiti": compiti.len(),
        "parole_mediana": mediana,
        "quote": { "criterio_fine": quota("criterio_fine", true), "vincoli": quota("vincoli", true), "perche": quota("perche", true), "contesto": quota("contesto", true) },
        "per_progetto": per_progetto.iter().map(|(k, (n, c))| json!({"progetto": k, "compiti": n, "con_verifica": c})).collect::<Vec<_>>(),
    })
}
