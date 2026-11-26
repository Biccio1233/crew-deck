//! Riassunti incrementali delle sessioni con Haiku: righe «su cosa» ed «esito o domanda».
//! Metodo (non codice) da billion-context, `kernel/src/compression-rules.ts`, MIT.
use crate::carteggio::{con_tempo, estrai_json};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Sender};
use std::sync::{Arc, Mutex};

/// Interruttore «Riassunti automatici»: spento all'avvio, lo accende l'interfaccia se l'utente ha detto sì.
pub static RIASSUNTI: AtomicBool = AtomicBool::new(false);

pub fn acceso() -> bool {
    RIASSUNTI.load(Ordering::Relaxed)
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct Riassunto {
    pub su_cosa: String,
    pub esito: String,
    /// `ultima_attivita` della sessione già riassunta.
    pub fino_a: i64,
}

/// Porta a mano la regex `SENSIBILE` di `giudice/giudice.py` (stessi termini, senza maiuscole, `\b` per pwd, pin, puk, otp)
/// per non aggiungere la dipendenza `regex`. Se cambia là, va cambiata qui.
pub fn sensibile(testo: &str) -> bool {
    const OVUNQUE: [&str; 9] = ["passw", "password", "credenzial", "seed phrase", "frase segreta", "codice di accesso", "codice segreto", "chiave privata", "private key"];
    const PAROLE: [&str; 4] = ["pwd", "pin", "puk", "otp"];
    let t = testo.to_lowercase();
    if OVUNQUE.iter().any(|p| t.contains(p)) {
        return true;
    }
    // `\b` di Python: confine fra un carattere di parola (lettera, cifra, _) e uno che non lo è
    let di_parola = |c: Option<char>| c.is_some_and(|c| c.is_alphanumeric() || c == '_');
    PAROLE.iter().any(|p| t.match_indices(p).any(|(i, _)| !di_parola(t[..i].chars().next_back()) && !di_parola(t[i + p.len()..].chars().next())))
}

const ISTRUZIONI: &str = r#"Riassumi lo stato di una sessione di lavoro con un agente AI di programmazione (Claude Code, Codex, pi) in due righe brevi.
Ricevi: il riassunto precedente (se c'è), l'ultima richiesta dell'utente, la fine dell'ultima risposta dell'agente e, se c'è, il motivo per cui la sessione aspetta l'utente.
Regole:
- "su_cosa": argomento — esito, al massimo 8 parole (es. "App todo — tema scuro"). Descrive l'obiettivo com'è adesso: se la richiesta nuova cambia rotta, riscrivilo secondo l'obiettivo attuale, non tenere la prima frase. Se il riassunto precedente è ancora giusto, tienilo.
- "esito": al massimo 12 parole. Se la sessione aspetta l'utente, inizia con "aspetta: " e di' cosa le serve (la domanda o il permesso). Altrimenti di' cosa è uscito dal lavoro.
- Quando manca spazio: prima l'obiettivo e i vincoli, poi le decisioni, poi i dettagli.
- Scrivi nella lingua della conversazione (quella dell'utente).
- Non inventare fatti. Niente nomi di file lunghi, niente codice.
Rispondi SOLO con un oggetto JSON, senza testo prima o dopo:
{"su_cosa": "...", "esito": "..."}"#;

fn testa(t: &str, n: usize) -> String {
    t.chars().take(n).collect()
}

fn coda(t: &str, n: usize) -> String {
    let tot = t.chars().count();
    t.chars().skip(tot.saturating_sub(n)).collect()
}

/// Testo per Haiku. ponytail: si usano solo l'ultima richiesta e l'ultima risposta, la storia viaggia nel
/// riassunto precedente; se serve più contesto si raccolgono tutte le richieste del turno.
pub fn richiesta(prec: Option<&Riassunto>, richiesta: Option<&str>, risposta: Option<&str>, attesa: Option<&str>) -> String {
    let mut t = format!("{ISTRUZIONI}\n");
    if let Some(p) = prec {
        t.push_str(&format!("\n<riassunto_precedente>\nsu_cosa: {}\nesito: {}\n</riassunto_precedente>\n", p.su_cosa, p.esito));
    }
    t.push_str(&format!("\n<ultima_richiesta_utente>\n{}\n</ultima_richiesta_utente>\n", richiesta.map(|r| testa(r, 2000)).unwrap_or_else(|| "(nessuna)".into())));
    t.push_str(&format!("\n<fine_ultima_risposta_agente>\n{}\n</fine_ultima_risposta_agente>\n", risposta.map(|r| coda(r, 2500)).unwrap_or_else(|| "(nessuna)".into())));
    if let Some(a) = attesa {
        t.push_str(&format!("\n<la_sessione_aspetta>\n{a}\n</la_sessione_aspetta>\n"));
    }
    t
}

fn parole(t: &str, n: usize) -> String {
    t.split_whitespace().take(n).collect::<Vec<_>>().join(" ")
}

/// Le due righe dall'uscita di Haiku, tagliate a 8 e 12 parole. None se manca una delle due.
pub fn interpreta(uscita: &str) -> Option<(String, String)> {
    let v = estrai_json(uscita)?;
    let su_cosa = parole(v["su_cosa"].as_str()?, 8);
    let esito = parole(v["esito"].as_str()?, 12);
    (!su_cosa.is_empty() && !esito.is_empty()).then_some((su_cosa, esito))
}

/// La chiave (`claude:<id>`) come nome di file: solo lettere, cifre, `-`, `_` e `.`; il resto diventa `-`.
fn file_di(dir: &Path, chiave: &str) -> PathBuf {
    let nome: String = chiave.chars().map(|c| if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.') { c } else { '-' }).collect();
    dir.join(format!("{nome}.json"))
}

pub fn leggi(dir: &Path, chiave: &str) -> Option<Riassunto> {
    serde_json::from_str(&std::fs::read_to_string(file_di(dir, chiave)).ok()?).ok()
}

/// Scrittura atomica: file temporaneo accanto, poi rinomina.
pub fn scrivi(dir: &Path, chiave: &str, r: &Riassunto) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let file = file_di(dir, chiave);
    let temp = file.with_extension(format!("json.tmp-{}", std::process::id()));
    std::fs::write(&temp, serde_json::to_string_pretty(r).map_err(std::io::Error::other)?)
        .and_then(|_| std::fs::rename(&temp, &file))
        .inspect_err(|_| {
            let _ = std::fs::remove_file(&temp);
        })
}

pub struct Lavoro {
    pub chiave: String,
    pub richiesta: Option<String>,
    pub risposta: Option<String>,
    pub attesa: Option<String>,
    pub ultima_attivita: i64,
}

/// Coda dei riassunti: un solo thread, un lavoro alla volta; la vedetta manda e non aspetta mai.
pub struct Coda {
    tx: Sender<Lavoro>,
    /// Sessioni con un lavoro già in coda o in corso: non si accodano di nuovo.
    in_coda: Arc<Mutex<HashSet<String>>>,
}

impl Coda {
    pub fn avvia(dir: PathBuf) -> Coda {
        let (tx, rx) = channel::<Lavoro>();
        let in_coda = Arc::new(Mutex::new(HashSet::new()));
        let in_coda2 = in_coda.clone();
        std::thread::spawn(move || {
            for l in rx {
                if let Err(e) = esegui(&dir, &l) {
                    // un errore non si riprova: il prossimo passaggio di stato porta un lavoro nuovo
                    eprintln!("riassunto di {}: {e}", l.chiave);
                }
                in_coda2.lock().unwrap().remove(&l.chiave);
            }
        });
        Coda { tx, in_coda }
    }

    pub fn chiedi(&self, lavoro: Lavoro) {
        if !acceso() || !self.in_coda.lock().unwrap().insert(lavoro.chiave.clone()) {
            return;
        }
        let chiave = lavoro.chiave.clone();
        if self.tx.send(lavoro).is_err() {
            self.in_coda.lock().unwrap().remove(&chiave);
        }
    }
}

fn esegui(dir: &Path, l: &Lavoro) -> Result<(), String> {
    if !acceso() {
        return Ok(());
    }
    let prec = leggi(dir, &l.chiave);
    if prec.as_ref().is_some_and(|p| l.ultima_attivita <= p.fino_a) {
        return Ok(());
    }
    if l.richiesta.as_deref().is_some_and(sensibile) || l.risposta.as_deref().is_some_and(sensibile) {
        // niente invio; si toglie il riassunto vecchio così la riga torna sul ripiego
        let _ = std::fs::remove_file(file_di(dir, &l.chiave));
        return Ok(());
    }
    let testo = richiesta(prec.as_ref(), l.richiesta.as_deref(), l.risposta.as_deref(), l.attesa.as_deref());
    let mut cmd = Command::new("claude");
    cmd.args(["-p", "--tools", "", "--no-session-persistence", "--setting-sources", "", "--output-format", "text", "--model", "haiku"])
        .env("CREWDECK_GIUDICE", "0")
        .current_dir(std::env::temp_dir());
    let out = con_tempo(cmd, Some(&testo), 60)?;
    let (su_cosa, esito) = interpreta(&out).ok_or_else(|| format!("risposta non leggibile: {}", testa(&out, 200)))?;
    scrivi(dir, &l.chiave, &Riassunto { su_cosa, esito, fino_a: l.ultima_attivita }).map_err(|e| e.to_string())
}

#[cfg(test)]
mod prove {
    use super::*;

    #[test]
    fn sensibile_come_il_giudice() {
        assert!(sensibile("non ricordo la password dell'archivio"));
        assert!(sensibile("ho le pwd del server"));
        assert!(sensibile("il pin della carta"));
        assert!(sensibile("Il PIN della carta"));
        assert!(sensibile("la Chiave Privata del wallet"));
        assert!(!sensibile("sistema il login"));
        assert!(!sensibile("aggiungi la spinta al pinguino"));
        assert!(!sensibile("pwdx e pin_code"));
        // stessi esiti della regex di giudice.py (confini di parola Unicode, spazi singoli)
        for (t, atteso) in [("PIN:1234", true), ("spin", false), ("pin_x", false), ("x-pin-y", true), ("OTP", true), ("otp2", false), ("àpin", false), ("pinè", false), ("Seed  phrase", false), ("CODICE DI ACCESSO", true), ("codice  segreto", false), ("Passwort", true), ("pwd.", true), ("my_pwd", false)] {
            assert_eq!(sensibile(t), atteso, "{t}");
        }
    }

    #[test]
    fn richiesta_contiene_precedente_e_attesa() {
        let prec = Riassunto { su_cosa: "Crew Deck — notifiche".into(), esito: "notifiche provate".into(), fino_a: 1 };
        let t = richiesta(Some(&prec), Some("aggiungi i riassunti"), Some("Fatto."), Some("Ti fa una domanda"));
        assert!(t.contains("Crew Deck — notifiche"));
        assert!(t.contains("Ti fa una domanda"));
        assert!(t.contains("aggiungi i riassunti"));
    }

    #[test]
    fn richiesta_taglia_ingresso() {
        let risposta = format!("{}{}", "a".repeat(7500), "b".repeat(2500));
        let t = richiesta(None, Some(&"r".repeat(5000)), Some(&risposta), None);
        assert!(t.contains(&"b".repeat(2500)));
        assert!(!t.contains(&"a".repeat(10)));
        assert!(!t.contains(&"r".repeat(2001)));
        assert!(t.contains(&"r".repeat(2000)));
    }

    #[test]
    fn interpreta_json_con_recinti() {
        assert_eq!(interpreta("```json\n{\"su_cosa\":\"a b\",\"esito\":\"c\"}\n```"), Some(("a b".into(), "c".into())));
        assert_eq!(interpreta("non so"), None);
        let (s, e) = interpreta(r#"{"su_cosa":"uno due tre quattro cinque sei sette otto nove dieci undici dodici","esito":"1 2 3 4 5 6 7 8 9 10 11 12 13 14"}"#).unwrap();
        assert_eq!(s, "uno due tre quattro cinque sei sette otto");
        assert_eq!(e.split_whitespace().count(), 12);
        assert_eq!(interpreta(r#"{"su_cosa":"  ","esito":"x"}"#), None);
    }

    #[test]
    fn scrivi_e_leggi() {
        let dir = std::env::temp_dir().join(format!("crewdeck-riassunti-{}-{}", std::process::id(), crate::ora_ms()));
        let r = Riassunto { su_cosa: "Demo — prova".into(), esito: "aspetta: quale colore?".into(), fino_a: 42 };
        scrivi(&dir, "claude:abc-123", &r).unwrap();
        assert_eq!(leggi(&dir, "claude:abc-123"), Some(r));
        assert_eq!(leggi(&dir, "claude:altro"), None);
        let nomi: Vec<String> = std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().to_string()).collect();
        assert_eq!(nomi, vec!["claude-abc-123.json".to_string()]);
        std::fs::remove_dir_all(&dir).ok();
    }
}
