//! Trova i registri dei tre agenti e li tiene aggiornati in memoria.
use crate::adapters::Lettore;
use crate::engine::{applica_attesa, calcola, ruolo, Calcolo, Opzioni, Ruolo};
use crate::localizza::{localizza, Cercata, Posizione};
use crate::model::{Agente, Token};
use serde::Serialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

#[derive(Debug, Clone, Serialize)]
pub struct Vista {
    pub id: String,
    pub file: String,
    /// Dove gira adesso, se il processo è vivo.
    pub posizione: Option<Posizione>,
    /// Ultima app in cui l'ho vista girare (per riprenderla lì).
    pub ultima_app: Option<String>,
    pub agente: Agente,
    pub cartella: Option<String>,
    pub progetto: String,
    pub titolo: Option<String>,
    pub modello: Option<String>,
    pub richiesta: Option<String>,
    /// Coda dell'ultimo messaggio dell'agente (solo locale, per il carteggio).
    pub ultima_risposta: Option<String>,
    pub n_richieste: u32,
    pub inizio: Option<i64>,
    /// Inizio del turno in corso (ultima richiesta dell'utente), per distinguere i turni.
    pub inizio_turno: Option<i64>,
    pub ultima_attivita: i64,
    pub ruolo: Ruolo,
    pub token: Token,
    pub costo_usd: Option<f64>,
    pub errori: u32,
    pub origine: Option<String>,
    pub automatica: bool,
    /// Chiusa dall'utente nella vista (i file restano; ricompare se torna attiva).
    pub chiusa: bool,
    /// Riga «su cosa» dell'ultimo riassunto salvato (la riempie lib.rs prima di mandare le viste).
    pub su_cosa: Option<String>,
    /// Riga «esito o domanda» dell'ultimo riassunto salvato.
    pub esito: Option<String>,
    #[serde(flatten)]
    pub calcolo: Calcolo,
}

pub struct Scanner {
    radici: Vec<(Agente, PathBuf)>,
    lettori: HashMap<PathBuf, Lettore>,
    posizioni: HashMap<String, Posizione>,
    ultima_app: HashMap<String, String>,
    localizzato: Option<Instant>,
    /// chiave → ultima attività al momento della chiusura (persistito in ~/.crew-deck/chiuse.json).
    chiuse: HashMap<String, i64>,
    /// Dove si salvano le chiuse; None nei test (niente scritture fuori).
    file_chiuse: Option<PathBuf>,
    /// Si guardano solo i file toccati negli ultimi N giorni.
    finestra: Duration,
    /// Segnali «ti aspetta» scritti dall'hook di Claude Code (~/.crew-deck/attese/<sessione>); None nei test.
    pub cartella_attese: Option<PathBuf>,
}

impl Scanner {
    pub fn new(home: &Path) -> Self {
        Scanner {
            radici: vec![
                (Agente::Claude, home.join(".claude/projects")),
                (Agente::Codex, home.join(".codex/sessions")),
                (Agente::Pi, home.join(".pi/agent/sessions")),
            ],
            lettori: HashMap::new(),
            posizioni: HashMap::new(),
            ultima_app: HashMap::new(),
            localizzato: None,
            chiuse: carica_chiuse(&home.join(".crew-deck/chiuse.json")),
            file_chiuse: Some(home.join(".crew-deck/chiuse.json")),
            finestra: Duration::from_secs(3 * 24 * 3600),
            cartella_attese: Some(home.join(".crew-deck/attese")),
        }
    }

    pub fn con_radici(radici: Vec<(Agente, PathBuf)>, finestra: Duration) -> Self {
        Scanner { radici, lettori: HashMap::new(), posizioni: HashMap::new(), ultima_app: HashMap::new(), localizzato: None, chiuse: HashMap::new(), file_chiuse: None, finestra, cartella_attese: None }
    }

    /// Cerca file nuovi e legge le code dei file già noti.
    pub fn aggiorna(&mut self) {
        let limite = SystemTime::now() - self.finestra;
        for (agente, radice) in self.radici.clone() {
            let mut trovati = Vec::new();
            raccogli(&radice, &mut trovati, 0);
            for f in trovati {
                let recente = std::fs::metadata(&f).and_then(|m| m.modified()).map(|t| t >= limite).unwrap_or(false);
                if !recente && !self.lettori.contains_key(&f) {
                    continue;
                }
                let l = self.lettori.entry(f.clone()).or_insert_with(|| Lettore::new(agente, f.clone()));
                let _ = l.aggiorna();
            }
        }
        // dove gira ogni sessione: ogni 5 secondi basta (ps + lsof costano qualche decina di ms)
        if self.localizzato.map(|t| t.elapsed() > Duration::from_secs(5)).unwrap_or(true) {
            self.localizza_ora();
        }
    }

    pub fn localizza_ora(&mut self) {
        let cercate: Vec<Cercata> = self
            .lettori
            .values()
            .filter(|l| l.acc.ultima_attivita > 0 && !l.acc.automatica)
            .map(|l| Cercata { chiave: chiave(l.acc.agente, &l.acc.id), id: &l.acc.id, agente: l.acc.agente, cwd: l.acc.cwd.as_deref(), ultima_attivita: l.acc.ultima_attivita })
            .collect();
        self.posizioni = localizza(&cercate);
        for (k, p) in &self.posizioni {
            self.ultima_app.insert(k.clone(), p.app.clone());
        }
        self.localizzato = Some(Instant::now());
    }

    /// Toglie la sessione dalla vista. Non cancella nulla.
    pub fn chiudi(&mut self, agente: Agente, id: &str) -> bool {
        let Some(l) = self.lettori.values().find(|l| l.acc.agente == agente && l.acc.id == id) else { return false };
        self.chiuse.insert(chiave(agente, id), l.acc.ultima_attivita);
        salva_chiuse(self.file_chiuse.as_deref(), &self.chiuse);
        true
    }

    /// Rimette la sessione nella vista.
    pub fn ripristina(&mut self, agente: Agente, id: &str) {
        self.chiuse.remove(&chiave(agente, id));
        salva_chiuse(self.file_chiuse.as_deref(), &self.chiuse);
    }

    /// Una sessione chiusa resta chiusa finché non c'è attività nuova dopo la chiusura.
    fn e_chiusa(&self, k: &str, ultima_attivita: i64) -> bool {
        self.chiuse.get(k).map(|t| ultima_attivita <= *t).unwrap_or(false)
    }

    /// Ultima richiesta e ultima risposta di una sessione.
    pub fn conversazione(&self, agente: Agente, id: &str) -> Option<(Option<String>, Option<String>)> {
        let l = self.lettori.values().find(|l| l.acc.agente == agente && l.acc.id == id)?;
        Some((l.acc.ultimo_prompt.clone(), l.acc.ultimo_testo.clone()))
    }

    /// Contesto per il carteggio: posizione, ultimo messaggio dell'agente, cartella.
    pub fn contesto(&self, agente: Agente, id: &str) -> Option<(Option<Posizione>, Option<String>, Option<String>)> {
        let l = self.lettori.values().find(|l| l.acc.agente == agente && l.acc.id == id)?;
        Some((self.posizioni.get(&chiave(agente, id)).cloned(), l.acc.ultimo_testo.clone(), l.acc.cwd.clone()))
    }

    /// Dati per le azioni: posizione, file, cartella, ultima app.
    pub fn per_azione(&self, agente: Agente, id: &str) -> Option<(Option<Posizione>, String, Option<String>, Option<String>, Option<String>)> {
        let k = chiave(agente, id);
        let l = self.lettori.values().find(|l| l.acc.agente == agente && l.acc.id == id)?;
        Some((self.posizioni.get(&k).cloned(), l.acc.file.to_string_lossy().to_string(), l.acc.cwd.clone(), self.ultima_app.get(&k).cloned(), l.acc.origine.clone()))
    }

    pub fn viste(&self, ora: i64) -> Vec<Vista> {
        let o = Opzioni::default();
        let attese = leggi_attese(self.cartella_attese.as_deref());
        let mut v: Vec<Vista> = self
            .lettori
            .values()
            .filter(|l| l.acc.ultima_attivita > 0)
            .map(|l| {
                let a = &l.acc;
                let k = chiave(a.agente, &a.id);
                Vista {
                    id: a.id.clone(),
                    file: a.file.to_string_lossy().to_string(),
                    posizione: self.posizioni.get(&k).cloned(),
                    ultima_app: self.ultima_app.get(&k).cloned(),
                    agente: a.agente,
                    // in cmux il nome è quello dato allo spazio di lavoro nella colonna di cmux
                    progetto: self
                        .posizioni
                        .get(&k)
                        .and_then(|p| p.cmux_titolo.clone())
                        .unwrap_or_else(|| a.cwd.as_deref().map(progetto).unwrap_or_else(|| "?".into())),
                    cartella: a.cwd.clone(),
                    titolo: a.titolo.clone(),
                    modello: a.modello.clone(),
                    richiesta: a.ultimo_prompt.clone(),
                    ultima_risposta: if a.automatica { None } else { a.ultimo_testo.clone() },
                    n_richieste: a.n_prompt,
                    inizio: a.inizio,
                    inizio_turno: a.inizio_turno,
                    ultima_attivita: a.ultima_attivita,
                    ruolo: ruolo(a),
                    token: a.token.clone(),
                    costo_usd: a.costo_usd,
                    errori: a.errori,
                    origine: a.origine.clone(),
                    automatica: a.automatica,
                    chiusa: self.e_chiusa(&k, a.ultima_attivita),
                    su_cosa: None,
                    esito: None,
                    calcolo: {
                        let mut c = calcola(a, ora, &o);
                        // processo vivo in un terminale: la sessione è aperta e aspetta solo un messaggio, non è «in porto»
                        if c.stato == crate::engine::Stato::Inattivo && self.posizioni.contains_key(&k) {
                            c.stato = crate::engine::Stato::Finito;
                            c.motivo = crate::engine::Motivo::FineTurno;
                            c.stimato = false;
                            c.azione = "Aperta, pronta per un nuovo compito".into();
                        }
                        if let Some((motivo, segnale)) = attese.get(&a.id).filter(|_| a.agente == Agente::Claude) {
                            applica_attesa(&mut c, motivo, *segnale, mtime_ms(&a.file), self.posizioni.contains_key(&k), ora, &o);
                        }
                        c
                    },
                }
            })
            .collect();
        v.sort_by(|a, b| b.ultima_attivita.cmp(&a.ultima_attivita));
        v
    }
}

fn mtime_ms(p: &Path) -> i64 {
    std::fs::metadata(p)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// sessione → (motivo, quando): un file per sessione, contenuto «domanda» o «permesso».
fn leggi_attese(dir: Option<&Path>) -> HashMap<String, (String, i64)> {
    let mut m = HashMap::new();
    let Some(Ok(rd)) = dir.map(std::fs::read_dir) else { return m };
    for e in rd.flatten() {
        let p = e.path();
        let quando = mtime_ms(&p);
        // un segnale per sessione, scritto dall'hook: dopo due giorni non serve più
        if crate::ora_ms() - quando > 2 * 24 * 3_600_000 {
            let _ = std::fs::remove_file(&p);
            continue;
        }
        if let (Some(id), Ok(motivo)) = (p.file_name().and_then(|n| n.to_str()), std::fs::read_to_string(&p)) {
            m.insert(id.to_string(), (motivo.trim().to_string(), quando));
        }
    }
    m
}

fn raccogli(dir: &Path, out: &mut Vec<PathBuf>, prof: u8) {
    if prof > 5 {
        return;
    }
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        let Ok(ft) = e.file_type() else { continue };
        if ft.is_dir() {
            // i sotto-agenti di Claude stanno in <sessione>/subagents: si leggono in una fase successiva
            if p.file_name().map(|n| n == "subagents").unwrap_or(false) {
                continue;
            }
            raccogli(&p, out, prof + 1);
        } else if p.extension().map(|x| x == "jsonl").unwrap_or(false) {
            out.push(p);
        }
    }
}

/// Nome leggibile del progetto: l'ultima cartella del percorso.
/// Nella cartella home si scrive «Cartella home» invece del nome utente.
pub fn progetto(cwd: &str) -> String {
    let senza_barra = cwd.trim_end_matches(['/', '\\']);
    if dirs::home_dir().is_some_and(|h| h.to_string_lossy().trim_end_matches('/') == senza_barra) {
        return "Cartella home".into();
    }
    cwd.trim_end_matches(['/', '\\']).rsplit(['/', '\\']).next().unwrap_or(cwd).to_string()
}

pub fn chiave(agente: Agente, id: &str) -> String {
    format!("{:?}:{}", agente, id).to_lowercase()
}

fn carica_chiuse(f: &Path) -> HashMap<String, i64> {
    std::fs::read_to_string(f).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

fn salva_chiuse(f: Option<&Path>, m: &HashMap<String, i64>) {
    let Some(f) = f else { return };
    if let Some(d) = f.parent() {
        let _ = std::fs::create_dir_all(d);
    }
    let _ = std::fs::write(f, serde_json::to_string_pretty(m).unwrap_or_default());
}
