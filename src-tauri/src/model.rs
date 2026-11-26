//! Modello comune: ogni adattatore riempie un `Acc` leggendo il registro riga per riga.
use serde::Serialize;
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Agente {
    Claude,
    Codex,
    Pi,
}

/// Ultimo evento significativo del registro.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum Ev {
    #[default]
    Nessuno,
    Prompt,
    Pensa,
    Testo,
    Strumento,
    Risultato,
    FineTurno,
    Interrotto,
    ErroreApi,
}

#[derive(Debug, Clone, Serialize)]
pub struct Strumento {
    pub id: String,
    pub nome: String,
    pub azione: String,
    pub inizio: i64,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Token {
    pub input: u64,
    pub output: u64,
    pub cache_lettura: u64,
    pub cache_scrittura: u64,
}

/// Stato accumulato di una sessione. Tutti i tempi in millisecondi epoch.
#[derive(Debug, Clone)]
pub struct Acc {
    pub agente: Agente,
    pub id: String,
    pub file: PathBuf,
    pub cwd: Option<String>,
    pub titolo: Option<String>,
    pub modello: Option<String>,
    pub inizio: Option<i64>,
    pub ultima_attivita: i64,
    pub ultimo_prompt: Option<String>,
    /// Ultimo testo scritto dall'agente (coda), per dare contesto al giudice e alla riscrittura.
    pub ultimo_testo: Option<String>,
    /// L'ultimo testo intero (non solo la coda tenuta) parla di password o credenziali.
    pub ultimo_testo_sensibile: bool,
    pub n_prompt: u32,
    pub aperti: Vec<Strumento>,
    pub ultimo_evento: Ev,
    pub quando_ultimo_evento: i64,
    pub inizio_turno: Option<i64>,
    pub domanda_aperta: bool,
    pub token: Token,
    pub costo_usd: Option<f64>,
    /// Conteggio degli strumenti usati nel turno corrente, per il ruolo d'equipaggio.
    pub strumenti_turno: HashMap<String, u32>,
    pub ultima_azione: Option<String>,
    pub errori: u32,
    /// Da dove è partita: cli, claude-desktop, Codex Desktop, sdk-cli…
    pub origine: Option<String>,
    /// Lanciata da un altro programma (graphify, Floorplan, script), non da una persona.
    pub automatica: bool,
    /// Usage per id di messaggio (Claude ripete l'usage su ogni blocco dello stesso messaggio).
    pub usage_visti: HashMap<String, Token>,
}

impl Acc {
    pub fn new(agente: Agente, id: String, file: PathBuf) -> Self {
        Acc {
            agente,
            id,
            file,
            cwd: None,
            titolo: None,
            modello: None,
            inizio: None,
            ultima_attivita: 0,
            ultimo_prompt: None,
            ultimo_testo: None,
            ultimo_testo_sensibile: false,
            n_prompt: 0,
            aperti: Vec::new(),
            ultimo_evento: Ev::Nessuno,
            quando_ultimo_evento: 0,
            inizio_turno: None,
            domanda_aperta: false,
            token: Token::default(),
            costo_usd: None,
            strumenti_turno: HashMap::new(),
            ultima_azione: None,
            errori: 0,
            origine: None,
            automatica: false,
            usage_visti: HashMap::new(),
        }
    }

    pub fn testo_agente(&mut self, t: &str) {
        let t = t.trim();
        if !t.is_empty() {
            let n = t.chars().count();
            // si decide sul testo intero: una credenziale all'inizio di una risposta lunga sparisce dalla coda, il suo valore no
            self.ultimo_testo_sensibile = crate::riservatezza::sensibile(t);
            self.ultimo_testo = if n > 2000 {
                // il taglio può spezzare una chiave, un'email o un percorso, e il pezzo non lo riconosce più il filtro:
                // si toglie la parola tagliata (fino al primo spazio o a capo compreso)
                let coda: String = t.chars().skip(n - 2000).collect();
                coda.split_once(char::is_whitespace).map(|(_, resto)| resto.trim_start().to_string()).filter(|r| !r.is_empty())
            } else {
                Some(t.to_string())
            };
        }
    }

    pub fn tocca(&mut self, ts: i64) {
        if ts > self.ultima_attivita {
            self.ultima_attivita = ts;
        }
        if self.inizio.is_none() {
            self.inizio = Some(ts);
        }
    }

    pub fn evento(&mut self, ev: Ev, ts: i64) {
        self.ultimo_evento = ev;
        self.quando_ultimo_evento = ts;
        self.tocca(ts);
    }

    pub fn prompt(&mut self, testo: String, ts: i64) {
        self.n_prompt += 1;
        self.ultimo_prompt = Some(testo);
        self.inizio_turno = Some(ts);
        self.strumenti_turno.clear();
        self.aperti.clear();
        self.domanda_aperta = false;
        self.evento(Ev::Prompt, ts);
    }

    pub fn apri(&mut self, id: String, nome: String, azione: String, ts: i64) {
        if nome == "AskUserQuestion" || nome == "request_user_input" {
            self.domanda_aperta = true;
        }
        *self.strumenti_turno.entry(nome.clone()).or_insert(0) += 1;
        self.ultima_azione = Some(azione.clone());
        self.aperti.push(Strumento { id, nome, azione, inizio: ts });
        self.evento(Ev::Strumento, ts);
    }

    pub fn chiudi(&mut self, id: &str, errore: bool, ts: i64) {
        if let Some(pos) = self.aperti.iter().position(|s| s.id == id) {
            let s = self.aperti.remove(pos);
            if s.nome == "AskUserQuestion" || s.nome == "request_user_input" {
                self.domanda_aperta = false;
            }
        }
        if errore {
            self.errori += 1;
        }
        self.evento(Ev::Risultato, ts);
    }

    pub fn fine_turno(&mut self, ts: i64) {
        self.aperti.clear();
        self.domanda_aperta = false;
        self.evento(Ev::FineTurno, ts);
    }

    /// Registra l'usage di un messaggio; se lo stesso id ricompare, sostituisce invece di sommare.
    pub fn usage(&mut self, msg_id: Option<&str>, t: Token) {
        match msg_id {
            Some(id) => {
                if let Some(vecchio) = self.usage_visti.insert(id.to_string(), t.clone()) {
                    self.token.input = self.token.input.saturating_sub(vecchio.input);
                    self.token.output = self.token.output.saturating_sub(vecchio.output);
                    self.token.cache_lettura = self.token.cache_lettura.saturating_sub(vecchio.cache_lettura);
                    self.token.cache_scrittura = self.token.cache_scrittura.saturating_sub(vecchio.cache_scrittura);
                }
                self.somma(&t);
            }
            None => self.somma(&t),
        }
    }

    fn somma(&mut self, t: &Token) {
        self.token.input += t.input;
        self.token.output += t.output;
        self.token.cache_lettura += t.cache_lettura;
        self.token.cache_scrittura += t.cache_scrittura;
    }
}

/// Converte un timestamp RFC 3339 in millisecondi epoch.
pub fn ts_ms(v: Option<&serde_json::Value>) -> Option<i64> {
    let s = v?.as_str()?;
    chrono::DateTime::parse_from_rfc3339(s).ok().map(|d| d.timestamp_millis())
}

/// Accorcia un testo a una riga leggibile.
pub fn corto(s: &str, max: usize) -> String {
    let riga = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if riga.chars().count() <= max {
        riga
    } else {
        let mut t: String = riga.chars().take(max).collect();
        t.push('…');
        t
    }
}

/// Ultimo componente di un percorso, per dire "Modifica app.ts" invece del percorso intero.
pub fn nome_file(p: &str) -> String {
    p.rsplit(['/', '\\']).next().unwrap_or(p).to_string()
}

/// Come `corto`, ma la riservatezza si decide sul testo intero: se parla di password o credenziali
/// (anche dopo il taglio) l'etichetta non ne mostra niente, e dice «credenziali» così chi la legge dopo
/// (notifica, riassunto) la tratta da sensibile senza un campo in più.
pub fn corto_riservato(s: &str, max: usize) -> String {
    if crate::riservatezza::sensibile(s) { "(testo con credenziali, nascosto)".into() } else { corto(s, max) }
}

/// Frase leggibile per uno strumento, comune ai tre agenti.
pub fn azione_strumento(nome: &str, input: &serde_json::Value) -> String {
    let s = |k: &str| input.get(k).and_then(|v| v.as_str()).unwrap_or("");
    let n = nome.to_lowercase();
    match n.as_str() {
        "bash" | "shell" | "exec" | "exec_command" | "powershell" => {
            let d = s("description");
            if !d.is_empty() {
                format!("Esegue: {}", corto_riservato(d, 60))
            } else {
                let c = if s("command").is_empty() { s("cmd") } else { s("command") };
                format!("Esegue: {}", corto_riservato(c, 60))
            }
        }
        "read" => format!("Legge {}", nome_file(if s("file_path").is_empty() { s("path") } else { s("file_path") })),
        "edit" | "multiedit" | "apply_patch" => {
            let f = if s("file_path").is_empty() { s("path") } else { s("file_path") };
            if f.is_empty() { "Modifica file".into() } else { format!("Modifica {}", nome_file(f)) }
        }
        "write" => format!("Scrive {}", nome_file(if s("file_path").is_empty() { s("path") } else { s("file_path") })),
        "grep" | "glob" => format!("Cerca: {}", corto_riservato(if s("pattern").is_empty() { s("query") } else { s("pattern") }, 50)),
        "websearch" | "web_search" => format!("Cerca sul web: {}", corto_riservato(s("query"), 50)),
        "webfetch" | "web_fetch" => format!("Legge una pagina web: {}", corto_riservato(s("url"), 50)),
        "task" | "agent" => format!("Affida a un aiutante: {}", corto_riservato(s("description"), 50)),
        "askuserquestion" | "request_user_input" => "Ti fa una domanda".into(),
        "todowrite" => "Aggiorna la lista delle cose da fare".into(),
        "codemode" => "Esegue uno script di strumenti".into(),
        // mcp__server__strumento (Claude) → «Usa server: strumento»
        _ => match nome.strip_prefix("mcp__").and_then(|r| r.split_once("__")) {
            Some((server, strumento)) => format!("Usa {}: {}", server.replace("plugin_", "").replace('_', " "), strumento.replace('_', " ")),
            None => format!("Usa {}", nome),
        },
    }
}

#[cfg(test)]
mod prove {
    use super::*;

    #[test]
    fn coda_della_risposta_senza_frammento_tagliato() {
        // il taglio a 2000 caratteri cade dentro un percorso: il pezzo rimasto non si riconoscerebbe più
        let testo = format!("{} /Users/mario/progetti/x.rs {}", "a".repeat(480), "b".repeat(1990));
        let mut acc = Acc::new(Agente::Claude, "x".into(), "/Users/mario/s.jsonl".into());
        acc.testo_agente(&testo);
        assert_eq!(acc.ultimo_testo.as_deref(), Some("b".repeat(1990).as_str()));
        // testo corto: resta intero
        acc.testo_agente("Fatto: /Users/mario/x.rs");
        assert_eq!(acc.ultimo_testo.as_deref(), Some("Fatto: /Users/mario/x.rs"));
    }
}
