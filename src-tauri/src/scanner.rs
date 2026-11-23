//! Trova i registri dei tre agenti e li tiene aggiornati in memoria.
use crate::adapters::Lettore;
use crate::engine::{calcola, ruolo, Calcolo, Opzioni, Ruolo};
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
    pub n_richieste: u32,
    pub inizio: Option<i64>,
    pub ultima_attivita: i64,
    pub ruolo: Ruolo,
    pub token: Token,
    pub costo_usd: Option<f64>,
    pub errori: u32,
    pub origine: Option<String>,
    pub automatica: bool,
    #[serde(flatten)]
    pub calcolo: Calcolo,
}

pub struct Scanner {
    radici: Vec<(Agente, PathBuf)>,
    lettori: HashMap<PathBuf, Lettore>,
    posizioni: HashMap<String, Posizione>,
    ultima_app: HashMap<String, String>,
    localizzato: Option<Instant>,
    /// Si guardano solo i file toccati negli ultimi N giorni.
    finestra: Duration,
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
            finestra: Duration::from_secs(3 * 24 * 3600),
        }
    }

    pub fn con_radici(radici: Vec<(Agente, PathBuf)>, finestra: Duration) -> Self {
        Scanner { radici, lettori: HashMap::new(), posizioni: HashMap::new(), ultima_app: HashMap::new(), localizzato: None, finestra }
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

    /// Dati per le azioni: posizione, file, cartella, ultima app.
    pub fn per_azione(&self, agente: Agente, id: &str) -> Option<(Option<Posizione>, String, Option<String>, Option<String>)> {
        let k = chiave(agente, id);
        let l = self.lettori.values().find(|l| l.acc.agente == agente && l.acc.id == id)?;
        Some((self.posizioni.get(&k).cloned(), l.acc.file.to_string_lossy().to_string(), l.acc.cwd.clone(), self.ultima_app.get(&k).cloned()))
    }

    pub fn viste(&self, ora: i64) -> Vec<Vista> {
        let o = Opzioni::default();
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
                    progetto: a.cwd.as_deref().map(progetto).unwrap_or_else(|| "?".into()),
                    cartella: a.cwd.clone(),
                    titolo: a.titolo.clone(),
                    modello: a.modello.clone(),
                    richiesta: a.ultimo_prompt.clone(),
                    n_richieste: a.n_prompt,
                    inizio: a.inizio,
                    ultima_attivita: a.ultima_attivita,
                    ruolo: ruolo(a),
                    token: a.token.clone(),
                    costo_usd: a.costo_usd,
                    errori: a.errori,
                    origine: a.origine.clone(),
                    automatica: a.automatica,
                    calcolo: calcola(a, ora, &o),
                }
            })
            .collect();
        v.sort_by(|a, b| b.ultima_attivita.cmp(&a.ultima_attivita));
        v
    }
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
pub fn progetto(cwd: &str) -> String {
    cwd.trim_end_matches(['/', '\\']).rsplit(['/', '\\']).next().unwrap_or(cwd).to_string()
}

pub fn chiave(agente: Agente, id: &str) -> String {
    format!("{:?}:{}", agente, id).to_lowercase()
}
