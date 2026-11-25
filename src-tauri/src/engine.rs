//! Motore di stato e ruoli d'equipaggio. Funzioni pure: registro accumulato + ora → stato.
use crate::model::{Acc, Ev};
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Stato {
    /// 🚩 chiama lo skipper
    Aspetta,
    /// ⛵ in navigazione
    Lavora,
    /// 🏁 al traguardo
    Finito,
    /// ⚓ in porto
    Inattivo,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Motivo {
    Domanda,
    /// richiesta di permesso segnalata dall'hook di Claude Code (certa)
    Permesso,
    PermessoStimato,
    Strumento,
    Pensa,
    FineTurno,
    Interrotto,
    ErroreApi,
    Silenzio,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Ruolo {
    Tattico,
    Navigatore,
    Prodiere,
    Grinder,
    Randista,
    Timoniere,
}

#[derive(Debug, Clone, Serialize)]
pub struct Calcolo {
    pub stato: Stato,
    pub motivo: Motivo,
    pub stimato: bool,
    pub da_quando: i64,
    pub azione: String,
}

pub struct Opzioni {
    pub inattivo_dopo_ms: i64,
    pub soglia_permesso_ms: i64,
    pub pensa_max_ms: i64,
}

impl Default for Opzioni {
    fn default() -> Self {
        Opzioni { inattivo_dopo_ms: 30 * 60_000, soglia_permesso_ms: 10_000, pensa_max_ms: 10 * 60_000 }
    }
}

/// Segnale dell'hook `giudice/attesa.py`: Claude Code scrive la domanda nel registro solo dopo la risposta,
/// quindi «ti aspetta» arriva dall'hook. Vale finché il registro non si muove dopo il segnale
/// (tolleranza 2 s) e, a sessione chiusa, al massimo per il tempo di «in porto» × 4.
pub fn applica_attesa(c: &mut Calcolo, motivo: &str, segnale_ms: i64, registro_ms: i64, viva: bool, ora: i64, o: &Opzioni) {
    if registro_ms > segnale_ms + 2_000 || (!viva && ora - segnale_ms >= o.inattivo_dopo_ms * 4) {
        return;
    }
    let (m, azione) = match motivo {
        "domanda" => (Motivo::Domanda, "Ti fa una domanda".to_string()),
        "permesso" => (Motivo::Permesso, format!("Chiede il permesso: {}", c.azione.trim_start_matches("Forse chiede il permesso: "))),
        _ => return,
    };
    *c = Calcolo { stato: Stato::Aspetta, motivo: m, stimato: false, da_quando: segnale_ms, azione };
}

/// Strumenti che possono durare a lungo da soli: se sono fermi non vuol dire che chiedono un permesso.
fn lento(nome: &str) -> bool {
    let n = nome.to_lowercase();
    matches!(
        n.as_str(),
        "bash" | "shell" | "exec" | "exec_command" | "powershell" | "task" | "agent" | "websearch"
            | "web_search" | "webfetch" | "web_fetch" | "wait" | "codemode" | "skill" | "monitor"
    ) || ["wait", "render", "generate", "batch", "browser", "computer", "screenshot", "mcp__"]
        .iter()
        .any(|k| n.contains(k))
}

pub fn calcola(a: &Acc, ora: i64, o: &Opzioni) -> Calcolo {
    let silenzio = ora - a.ultima_attivita;
    let ultima_frase = || a.ultima_azione.clone().unwrap_or_else(|| "Nessuna attività".into());

    // 1. domanda all'utente ancora aperta: aspetta te (certo)
    if a.domanda_aperta && silenzio < o.inattivo_dopo_ms * 4 {
        let s = a.aperti.iter().rev().find(|s| s.nome == "AskUserQuestion" || s.nome == "request_user_input");
        return Calcolo {
            stato: Stato::Aspetta,
            motivo: Motivo::Domanda,
            stimato: false,
            da_quando: s.map(|s| s.inizio).unwrap_or(a.quando_ultimo_evento),
            azione: "Ti fa una domanda".into(),
        };
    }
    // 2. troppo silenzio: in porto
    if silenzio >= o.inattivo_dopo_ms {
        return Calcolo { stato: Stato::Inattivo, motivo: Motivo::Silenzio, stimato: true, da_quando: a.ultima_attivita, azione: ultima_frase() };
    }
    // 3. strumento in corso
    if let Some(s) = a.aperti.last() {
        let fermo = ora - s.inizio;
        if !lento(&s.nome) && fermo >= o.soglia_permesso_ms {
            return Calcolo {
                stato: Stato::Aspetta,
                motivo: Motivo::PermessoStimato,
                stimato: true,
                da_quando: s.inizio + o.soglia_permesso_ms,
                azione: format!("Forse chiede il permesso: {}", s.azione),
            };
        }
        return Calcolo {
            stato: Stato::Lavora,
            motivo: Motivo::Strumento,
            stimato: false,
            da_quando: a.inizio_turno.unwrap_or(s.inizio),
            azione: s.azione.clone(),
        };
    }
    // 4. niente in corso: decide l'ultimo evento
    let t = a.quando_ultimo_evento;
    match a.ultimo_evento {
        Ev::FineTurno => Calcolo { stato: Stato::Finito, motivo: Motivo::FineTurno, stimato: false, da_quando: t, azione: "Ha finito".into() },
        Ev::Interrotto => Calcolo { stato: Stato::Finito, motivo: Motivo::Interrotto, stimato: false, da_quando: t, azione: "Interrotto da te".into() },
        Ev::ErroreApi => Calcolo { stato: Stato::Finito, motivo: Motivo::ErroreApi, stimato: false, da_quando: t, azione: "Errore del servizio".into() },
        Ev::Nessuno => Calcolo { stato: Stato::Inattivo, motivo: Motivo::Silenzio, stimato: true, da_quando: a.ultima_attivita, azione: "Nessuna attività".into() },
        _ => {
            if silenzio >= o.pensa_max_ms {
                Calcolo { stato: Stato::Inattivo, motivo: Motivo::Silenzio, stimato: true, da_quando: a.ultima_attivita, azione: ultima_frase() }
            } else {
                let azione = match a.ultimo_evento {
                    Ev::Testo => "Scrive la risposta",
                    _ => "Pensa…",
                };
                Calcolo { stato: Stato::Lavora, motivo: Motivo::Pensa, stimato: false, da_quando: a.inizio_turno.unwrap_or(t), azione: azione.into() }
            }
        }
    }
}

/// Ruolo d'equipaggio dagli strumenti del turno corrente.
pub fn ruolo(a: &Acc) -> Ruolo {
    let mut conta = [0u32; 6]; // tattico, navigatore, prodiere, grinder, randista, timoniere
    for (nome, n) in &a.strumenti_turno {
        let i = match nome.to_lowercase().as_str() {
            "task" | "agent" | "spawn_agent" | "sendmessage" => 5,
            "websearch" | "web_search" | "webfetch" | "web_fetch" => 1,
            "read" | "grep" | "glob" | "ls" | "find" => 2,
            "edit" | "multiedit" | "write" | "apply_patch" | "notebookedit" => 3,
            "bash" | "shell" | "exec" | "exec_command" | "powershell" | "codemode" => 4,
            "todowrite" | "exitplanmode" | "askuserquestion" => 0,
            _ => continue,
        };
        conta[i] += n;
    }
    let ruoli = [Ruolo::Tattico, Ruolo::Navigatore, Ruolo::Prodiere, Ruolo::Grinder, Ruolo::Randista, Ruolo::Timoniere];
    let max = conta.iter().max().copied().unwrap_or(0);
    if max == 0 {
        return Ruolo::Tattico;
    }
    // a parità vince chi scrive: è il lavoro più visibile
    for i in [3, 5, 4, 2, 1, 0] {
        if conta[i] == max {
            return ruoli[i];
        }
    }
    Ruolo::Tattico
}
