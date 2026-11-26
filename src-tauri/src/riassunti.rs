//! Riassunti incrementali delle sessioni con Haiku: righe «su cosa» ed «esito o domanda».
//! Metodo (non codice) da billion-context, `kernel/src/compression-rules.ts`, MIT.
use crate::carteggio::{con_tempo, estrai_json};
use crate::riservatezza::{filtra, sensibile};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
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
    /// Revisione (`Lavoro::revisione`) della sessione già riassunta.
    pub fino_a: i64,
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

/// Come `richiesta`, ma ogni campo passa prima da `filtra` (i `FILTRI` di giudice.py), poi si taglia:
/// un percorso tagliato a metà non verrebbe più riconosciuto. Se il filtro fallisce non parte niente.
fn richiesta_oscurata(prec: Option<&Riassunto>, l: &Lavoro, filtra: impl Fn(&str) -> Result<String, String>) -> Result<String, String> {
    let prec = match prec {
        Some(p) => Some(Riassunto { su_cosa: filtra(&p.su_cosa)?, esito: filtra(&p.esito)?, fino_a: p.fino_a }),
        None => None,
    };
    let campo = |t: &Option<String>| t.as_deref().map(&filtra).transpose();
    Ok(richiesta(prec.as_ref(), campo(&l.richiesta)?.as_deref(), campo(&l.risposta)?.as_deref(), campo(&l.attesa)?.as_deref()))
}

/// Con la sessione in attesa l'esito comincia sempre con «aspetta: » minuscolo, e resta entro le 12 parole.
fn con_aspetta(esito: String, attesa: bool) -> String {
    if !attesa {
        return esito;
    }
    let t = esito.trim_start();
    let resto = match t.get(..8) {
        Some(p) if p.eq_ignore_ascii_case("aspetta:") => &t[8..],
        _ => t,
    };
    parole(&format!("aspetta: {}", resto.trim_start()), 12)
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
    /// `max(ultima_attivita, da_quando)`: il segnale dell'hook sposta `da_quando` senza muovere il registro.
    pub revisione: i64,
    /// La risposta intera parlava di password o credenziali: la coda in `risposta` può non dirlo.
    pub risposta_sensibile: bool,
}

/// Lavori in attesa, uno per sessione: chi arriva dopo prende il posto di quello che non è ancora partito.
#[derive(Default)]
struct InAttesa(HashMap<String, Lavoro>);

impl InAttesa {
    /// Vero se la sessione non aveva un lavoro in attesa (quindi la sua chiave va mandata al thread).
    fn metti(&mut self, l: Lavoro) -> bool {
        self.0.insert(l.chiave.clone(), l).is_none()
    }

    fn prendi(&mut self, chiave: &str) -> Option<Lavoro> {
        self.0.remove(chiave)
    }
}

/// Coda dei riassunti: un solo thread, un lavoro alla volta; la vedetta manda e non aspetta mai.
pub struct Coda {
    /// Il canale porta solo le chiavi; il lavoro, sempre il più recente, sta in `in_attesa`.
    tx: Sender<String>,
    in_attesa: Arc<Mutex<InAttesa>>,
}

impl Coda {
    pub fn avvia(dir: PathBuf) -> Coda {
        let (tx, rx) = channel::<String>();
        let in_attesa = Arc::new(Mutex::new(InAttesa::default()));
        let in_attesa2 = in_attesa.clone();
        std::thread::spawn(move || {
            for chiave in rx {
                let Some(l) = in_attesa2.lock().unwrap().prendi(&chiave) else { continue };
                if let Err(e) = esegui(&dir, &l) {
                    // un errore non si riprova: il prossimo passaggio di stato porta un lavoro nuovo
                    eprintln!("riassunto di {}: {e}", l.chiave);
                }
            }
        });
        Coda { tx, in_attesa }
    }

    pub fn chiedi(&self, lavoro: Lavoro) {
        if !acceso() {
            return;
        }
        let chiave = lavoro.chiave.clone();
        if self.in_attesa.lock().unwrap().metti(lavoro) {
            let _ = self.tx.send(chiave);
        }
    }
}

/// Lavoro già fatto, o con testo sensibile (richiesta, risposta o motivo dell'attesa, che per un permesso
/// contiene l'inizio del comando). Nel secondo caso si toglie il riassunto vecchio: la riga torna sul ripiego.
fn da_saltare(dir: &Path, l: &Lavoro) -> bool {
    if leggi(dir, &l.chiave).is_some_and(|p| l.revisione <= p.fino_a) {
        return true;
    }
    if l.risposta_sensibile || [&l.richiesta, &l.risposta, &l.attesa].iter().any(|t| t.as_deref().is_some_and(sensibile)) {
        let _ = std::fs::remove_file(file_di(dir, &l.chiave));
        return true;
    }
    false
}

fn esegui(dir: &Path, l: &Lavoro) -> Result<(), String> {
    if !acceso() || da_saltare(dir, l) {
        return Ok(());
    }
    let prec = leggi(dir, &l.chiave);
    // fail closed: senza filtro (python3 mancante o in errore) non si invia e la riga resta com'è
    let testo = richiesta_oscurata(prec.as_ref(), l, filtra).map_err(|e| format!("filtro non disponibile, non invio: {e}"))?;
    // il consenso si ricontrolla al confine dell'invio: l'interruttore può essere stato spento mentre il lavoro aspettava o si filtrava
    if !acceso() {
        return Ok(());
    }
    let mut cmd = Command::new("claude");
    cmd.args(["-p", "--tools", "", "--no-session-persistence", "--setting-sources", "", "--output-format", "text", "--model", "haiku"])
        .env("CREWDECK_GIUDICE", "0")
        .current_dir(std::env::temp_dir());
    let out = con_tempo(cmd, Some(&testo), 60)?;
    let (su_cosa, esito) = interpreta(&out).ok_or_else(|| format!("risposta non leggibile: {}", testa(&out, 200)))?;
    let esito = con_aspetta(esito, l.attesa.is_some());
    scrivi(dir, &l.chiave, &Riassunto { su_cosa, esito, fino_a: l.revisione }).map_err(|e| e.to_string())
}

#[cfg(test)]
mod prove {
    use super::*;

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

    fn lavoro(chiave: &str, revisione: i64, attesa: Option<&str>) -> Lavoro {
        Lavoro { chiave: chiave.into(), richiesta: Some("sistema il login".into()), risposta: Some("Fatto.".into()), attesa: attesa.map(String::from), revisione, risposta_sensibile: false }
    }

    #[test]
    fn risposta_lunga_con_password_all_inizio_non_parte() {
        // la coda tenuta (ultimi 2000 caratteri) non dice più «password»: si decide sul testo intero
        let mut acc = crate::model::Acc::new(crate::model::Agente::Claude, "x".into(), "/Users/mario/s.jsonl".into());
        acc.testo_agente(&format!("Mi serve la password del database: eccola, poi continuo. {}", "a".repeat(3000)));
        let risposta = acc.ultimo_testo.clone();
        assert!(!risposta.as_deref().is_some_and(sensibile));
        let l = Lavoro { risposta, risposta_sensibile: acc.ultimo_testo_sensibile, ..lavoro("claude:lunga", 5, None) };
        let dir = std::env::temp_dir().join(format!("crewdeck-lunga-{}-{}", std::process::id(), crate::ora_ms()));
        assert!(da_saltare(&dir, &l));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn richiesta_oscurata_filtra_ogni_campo_prima_del_taglio() {
        let prec = Riassunto { su_cosa: "Demo /Users/mario".into(), esito: "scritto a /Users/mario".into(), fino_a: 1 };
        // il percorso sta proprio dove `coda` taglia: va oscurato prima, non dopo
        let risposta = format!("{}/Users/mario/progetti/x.rs{}", "a".repeat(100), "b".repeat(2490));
        let l = Lavoro { chiave: "claude:x".into(), richiesta: Some("apri /Users/mario/y".into()), risposta: Some(risposta), attesa: Some("Chiede il permesso: cat /Users/mario/z".into()), revisione: 5, risposta_sensibile: false };
        let finto = |t: &str| Ok(t.replace("/Users/mario", "<percorso>"));
        let t = richiesta_oscurata(Some(&prec), &l, finto).unwrap();
        assert!(!t.contains("mario"), "{t}");
        assert!(!t.contains("rs/"), "{t}");
        assert_eq!(t.matches("<percorso>").count(), 4);
        // il filtro non va (python3 manca): non parte niente
        assert!(richiesta_oscurata(Some(&prec), &l, |_| Err("python3 non c'è".into())).is_err());
    }

    #[test]
    fn esito_in_attesa_comincia_con_aspetta() {
        assert_eq!(con_aspetta("Aspetta: modifica di auth.ts".into(), true), "aspetta: modifica di auth.ts");
        assert_eq!(con_aspetta("ASPETTA:quale colore?".into(), true), "aspetta: quale colore?");
        assert_eq!(con_aspetta("vuole sapere il limite dei rimborsi".into(), true), "aspetta: vuole sapere il limite dei rimborsi");
        assert_eq!(con_aspetta("1 2 3 4 5 6 7 8 9 10 11 12".into(), true).split_whitespace().count(), 12);
        assert_eq!(con_aspetta("Aspetta: niente".into(), false), "Aspetta: niente");
    }

    #[test]
    fn gia_riassunto_si_salta_per_revisione() {
        let dir = std::env::temp_dir().join(format!("crewdeck-revisione-{}-{}", std::process::id(), crate::ora_ms()));
        scrivi(&dir, "claude:r", &Riassunto { su_cosa: "Demo — login".into(), esito: "fatto".into(), fino_a: 100 }).unwrap();
        assert!(da_saltare(&dir, &lavoro("claude:r", 100, None)));
        // il segnale dell'hook sposta da_quando (quindi la revisione) senza muovere il registro: si rifà
        assert!(!da_saltare(&dir, &lavoro("claude:r", 150, Some("Ti fa una domanda"))));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn attesa_sensibile_non_parte_e_toglie_il_riassunto() {
        let dir = std::env::temp_dir().join(format!("crewdeck-attesa-{}-{}", std::process::id(), crate::ora_ms()));
        scrivi(&dir, "claude:p", &Riassunto { su_cosa: "Demo — login".into(), esito: "fatto".into(), fino_a: 1 }).unwrap();
        // se partisse davvero, servirebbe `claude`: qui deve fermarsi prima, senza errori
        let l = lavoro("claude:p", 5, Some("Chiede il permesso: Esegue: export DB_PASSWORD=x"));
        assert_eq!(da_saltare(&dir, &l), true);
        assert_eq!(leggi(&dir, "claude:p"), None);
        assert_eq!(da_saltare(&dir, &lavoro("claude:q", 5, Some("Ti fa una domanda"))), false);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn in_attesa_si_sostituisce_col_piu_recente() {
        let mut p = InAttesa::default();
        assert!(p.metti(lavoro("claude:a", 1, None)));
        assert!(!p.metti(lavoro("claude:a", 2, Some("Ti fa una domanda"))));
        assert!(p.metti(lavoro("claude:b", 3, None)));
        let a = p.prendi("claude:a").unwrap();
        assert_eq!((a.revisione, a.attesa.as_deref()), (2, Some("Ti fa una domanda")));
        assert!(p.prendi("claude:a").is_none());
        // preso (in corso): un lavoro nuovo si accoda di nuovo
        assert!(p.metti(lavoro("claude:a", 4, None)));
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
