//! Rilevatore dei passaggi di stato: confronta le viste di un giro di scansione con quelle del giro
//! precedente e dice quali eventi meritano una notifica. Logica pura, nessun I/O.
use crate::engine::{Motivo, Stato};
use crate::scanner::{chiave, Vista};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tipo {
    Domanda,
    Permesso,
    Finito,
    Errore,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Evento {
    pub chiave: String,
    pub tipo: Tipo,
    pub progetto: String,
    pub motivo: String,
}

/// Il permesso stimato diventa notifica solo dopo questo tempo di strumento fermo.
const PERMESSO_STIMATO_MS: i64 = 30_000;
/// Sotto questa durata un turno finito non merita una notifica.
const TURNO_LUNGO_MS: i64 = 120_000;
/// Il `da_quando` del permesso stimato è già inizio strumento + soglia del motore (10 s).
const SOGLIA_MOTORE_MS: i64 = 10_000;

#[derive(Default)]
pub struct Memoria {
    primo_giro_fatto: bool,
    /// Per sessione: stato e motivo del giro precedente.
    precedente: HashMap<String, (Stato, Motivo)>,
    /// Per sessione: turno (`inizio_turno`) dell'ultimo permesso notificato.
    permesso_notificato: HashMap<String, i64>,
}

impl Memoria {
    pub fn passaggi(&mut self, viste: &[Vista], ora: i64) -> Vec<Evento> {
        let mut eventi = Vec::new();
        let mut adesso = HashMap::new();
        for v in viste.iter().filter(|v| !v.automatica && !v.chiusa) {
            let k = chiave(v.agente, &v.id);
            let c = &v.calcolo;
            adesso.insert(k.clone(), (c.stato, c.motivo));
            let Some(&(s0, m0)) = self.precedente.get(&k).filter(|_| self.primo_giro_fatto) else { continue };
            // ponytail: senza `inizio_turno` (sessione senza richieste) vale 0, quindi un solo permesso in tutto; per superarlo serve un id di turno vero
            let turno = v.inizio_turno.unwrap_or(0);
            let tipo = match (c.stato, c.motivo) {
                (Stato::Aspetta, Motivo::Domanda) if (s0, m0) != (Stato::Aspetta, Motivo::Domanda) => Some(Tipo::Domanda),
                (Stato::Aspetta, Motivo::Permesso) if (s0, m0) != (Stato::Aspetta, Motivo::Permesso) && self.permesso_notificato.get(&k) != Some(&turno) => Some(Tipo::Permesso),
                (Stato::Aspetta, Motivo::PermessoStimato) if ora - (c.da_quando - SOGLIA_MOTORE_MS) >= PERMESSO_STIMATO_MS && self.permesso_notificato.get(&k) != Some(&turno) => Some(Tipo::Permesso),
                (Stato::Finito, Motivo::FineTurno) if matches!(s0, Stato::Lavora | Stato::Aspetta) && v.inizio_turno.is_some_and(|t| c.da_quando - t > TURNO_LUNGO_MS) => Some(Tipo::Finito),
                (Stato::Finito, Motivo::ErroreApi) if matches!(s0, Stato::Lavora | Stato::Aspetta) => Some(Tipo::Errore),
                _ => None,
            };
            if let Some(tipo) = tipo {
                if tipo == Tipo::Permesso {
                    self.permesso_notificato.insert(k.clone(), turno);
                }
                eventi.push(Evento { chiave: k, tipo, progetto: v.progetto.clone(), motivo: c.azione.clone() });
            }
        }
        self.permesso_notificato.retain(|k, _| adesso.contains_key(k));
        self.precedente = adesso;
        self.primo_giro_fatto = true;
        eventi
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::Calcolo;
    use crate::model::{Agente, Token};
    use crate::engine::Ruolo;

    fn vista(id: &str, stato: Stato, motivo: Motivo, da_quando: i64, inizio_turno: Option<i64>) -> Vista {
        Vista {
            id: id.into(),
            file: String::new(),
            posizione: None,
            ultima_app: None,
            agente: Agente::Claude,
            cartella: None,
            progetto: "demo".into(),
            titolo: None,
            modello: None,
            richiesta: None,
            ultima_risposta: None,
            n_richieste: 1,
            inizio: None,
            inizio_turno,
            ultima_attivita: da_quando,
            ruolo: Ruolo::Tattico,
            token: Token::default(),
            costo_usd: None,
            errori: 0,
            origine: None,
            automatica: false,
            chiusa: false,
            calcolo: Calcolo { stato, motivo, stimato: false, da_quando, azione: "azione".into() },
        }
    }

    fn lavora(t: i64) -> Vista {
        vista("a", Stato::Lavora, Motivo::Pensa, t, Some(t))
    }

    #[test]
    fn primo_giro_non_emette() {
        let mut m = Memoria::default();
        assert!(m.passaggi(&[vista("a", Stato::Aspetta, Motivo::Domanda, 0, Some(0))], 0).is_empty());
    }

    #[test]
    fn domanda_emette_una_volta() {
        let mut m = Memoria::default();
        let d = vista("a", Stato::Aspetta, Motivo::Domanda, 5, Some(0));
        m.passaggi(&[lavora(0)], 1);
        let e = m.passaggi(&[d.clone()], 6);
        assert_eq!(e.len(), 1);
        assert_eq!(e[0].tipo, Tipo::Domanda);
        assert_eq!(e[0].motivo, "azione");
        assert!(m.passaggi(&[d], 7).is_empty());
    }

    #[test]
    fn permessi_a_raffica_uno_per_turno() {
        let mut m = Memoria::default();
        let p = |t| vista("a", Stato::Aspetta, Motivo::Permesso, 5, Some(t));
        m.passaggi(&[lavora(0)], 1);
        let mut n = 0;
        for i in 0..10 {
            n += m.passaggi(&[p(0)], 10 + i * 2).len();
            n += m.passaggi(&[vista("a", Stato::Lavora, Motivo::Strumento, 0, Some(0))], 11 + i * 2).len();
        }
        assert_eq!(n, 1);
        m.passaggi(&[vista("a", Stato::Lavora, Motivo::Strumento, 100, Some(100))], 200);
        let e = m.passaggi(&[p(100)], 201);
        assert_eq!(e.len(), 1);
        assert_eq!(e[0].tipo, Tipo::Permesso);
    }

    #[test]
    fn permesso_stimato_aspetta_30_secondi() {
        let mut m = Memoria::default();
        let inizio_strumento = 1_000_000;
        let stimato = vista("a", Stato::Aspetta, Motivo::PermessoStimato, inizio_strumento + 10_000, Some(inizio_strumento));
        m.passaggi(&[lavora(inizio_strumento)], inizio_strumento + 1);
        assert!(m.passaggi(&[stimato.clone()], inizio_strumento + 20_000).is_empty());
        let e = m.passaggi(&[stimato], inizio_strumento + 31_000);
        assert_eq!(e.len(), 1);
        assert_eq!(e[0].tipo, Tipo::Permesso);
    }

    #[test]
    fn finito_breve_non_emette() {
        let mut m = Memoria::default();
        m.passaggi(&[lavora(0)], 1);
        assert!(m.passaggi(&[vista("a", Stato::Finito, Motivo::FineTurno, 30_000, Some(0))], 31_000).is_empty());
    }

    #[test]
    fn finito_lungo_emette() {
        let mut m = Memoria::default();
        m.passaggi(&[lavora(0)], 1);
        let e = m.passaggi(&[vista("a", Stato::Finito, Motivo::FineTurno, 121_000, Some(0))], 122_000);
        assert_eq!(e.len(), 1);
        assert_eq!(e[0].tipo, Tipo::Finito);
    }

    #[test]
    fn errore_emette() {
        let mut m = Memoria::default();
        m.passaggi(&[lavora(0)], 1);
        let e = m.passaggi(&[vista("a", Stato::Finito, Motivo::ErroreApi, 10, Some(0))], 11);
        assert_eq!(e.len(), 1);
        assert_eq!(e[0].tipo, Tipo::Errore);
    }

    #[test]
    fn interrotto_non_emette() {
        let mut m = Memoria::default();
        m.passaggi(&[lavora(0)], 1);
        assert!(m.passaggi(&[vista("a", Stato::Finito, Motivo::Interrotto, 500_000, Some(0))], 500_001).is_empty());
    }

    #[test]
    fn porto_a_finito_non_emette() {
        let mut m = Memoria::default();
        m.passaggi(&[vista("a", Stato::Inattivo, Motivo::Silenzio, 0, Some(0))], 1);
        assert!(m.passaggi(&[vista("a", Stato::Finito, Motivo::FineTurno, 500_000, Some(0))], 500_001).is_empty());
    }

    #[test]
    fn automatica_e_chiusa_ignorate() {
        let mut m = Memoria::default();
        let mut a = vista("a", Stato::Aspetta, Motivo::Domanda, 5, Some(0));
        a.automatica = true;
        let mut c = vista("b", Stato::Aspetta, Motivo::Domanda, 5, Some(0));
        c.chiusa = true;
        m.passaggi(&[vista("a", Stato::Lavora, Motivo::Pensa, 0, Some(0)), vista("b", Stato::Lavora, Motivo::Pensa, 0, Some(0))], 1);
        assert!(m.passaggi(&[a, c], 6).is_empty());
    }
}
