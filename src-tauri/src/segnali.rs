//! Rilevatore dei passaggi di stato: confronta le viste di un giro di scansione con quelle del giro
//! precedente e dice quali eventi meritano una notifica. Logica pura, nessun I/O.
use crate::engine::{Motivo, Stato, SOGLIA_PERMESSO_MS};
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
    /// Falso per i turni brevi finiti: niente notifica, ma il riassunto sì.
    pub notifica: bool,
}

/// Il permesso stimato diventa notifica solo dopo questo tempo di strumento fermo.
const PERMESSO_STIMATO_MS: i64 = 30_000;
/// Sotto questa durata un turno finito non merita una notifica.
const TURNO_LUNGO_MS: i64 = 120_000;
/// Una fine vera si vede entro un giro; un Finito più vecchio è un Inattivo convertito dallo scanner.
const FINE_FRESCA_MS: i64 = 60_000;

#[derive(Default)]
pub struct Memoria {
    primo_giro_fatto: bool,
    /// Per sessione: stato, motivo, `da_quando` e `inizio_turno` del giro precedente.
    precedente: HashMap<String, (Stato, Motivo, i64, Option<i64>)>,
    /// Per sessione: turno (`inizio_turno`) dell'ultimo permesso notificato.
    permesso_notificato: HashMap<String, i64>,
    /// Per sessione: l'ultima attesa di permesso (`inizio_turno`, `da_quando`) che ha già dato un evento.
    permesso_emesso: HashMap<String, (Option<i64>, i64)>,
    /// Sessioni già in attesa di permesso al primo giro: quell'attesa (`inizio_turno`, `da_quando`) non si notifica.
    attesa_ignorata: HashMap<String, (Option<i64>, i64)>,
}

impl Memoria {
    pub fn passaggi(&mut self, viste: &[Vista], ora: i64) -> Vec<Evento> {
        let mut eventi = Vec::new();
        let mut adesso = HashMap::new();
        for v in viste.iter().filter(|v| !v.automatica && !v.chiusa) {
            let k = chiave(v.agente, &v.id);
            let c = &v.calcolo;
            adesso.insert(k.clone(), (c.stato, c.motivo, c.da_quando, v.inizio_turno));
            let in_permesso = c.stato == Stato::Aspetta && matches!(c.motivo, Motivo::Permesso | Motivo::PermessoStimato);
            let attesa = (v.inizio_turno, c.da_quando);
            // l'attesa ignorata è una sola: finita quella (o arrivata un'altra), si torna a notificare
            if self.attesa_ignorata.get(&k).is_some_and(|a| !in_permesso || *a != attesa) {
                self.attesa_ignorata.remove(&k);
            }
            // primo giro: base silenziosa. Una sessione vista per la prima volta dopo parte da «in porto»,
            // così un'attesa già aperta si notifica con le regole di sempre
            let (s0, m0, d0, t0) = match self.precedente.get(&k) {
                _ if !self.primo_giro_fatto => {
                    if in_permesso {
                        self.attesa_ignorata.insert(k, attesa);
                    }
                    continue;
                }
                Some(&p) => p,
                None => (Stato::Inattivo, Motivo::Silenzio, 0, None),
            };
            let ignorata = self.attesa_ignorata.contains_key(&k);
            // ponytail: senza `inizio_turno` (sessione senza richieste) vale 0, quindi una sola notifica di permesso in tutto; per superarla serve un id di turno vero
            let turno = v.inizio_turno.unwrap_or(0);
            // a parità di stato e motivo, un `da_quando` nuovo è un'altra attesa o un altro turno arrivato fra due letture
            // (per la domanda solo se più recente: dal segnale dell'hook al registro `da_quando` torna indietro)
            let nuova_domanda = (s0, m0) != (Stato::Aspetta, Motivo::Domanda) || c.da_quando > d0;
            // ogni attesa di permesso nuova dà un evento (il riassunto si aggiorna); la notifica resta una per turno
            let nuovo_permesso = self.permesso_emesso.get(&k) != Some(&attesa);
            let prima_del_turno = self.permesso_notificato.get(&k) != Some(&turno);
            // fra due fini serve un turno nuovo: end_turn e turn_duration, o gli api_error a ogni tentativo, sono lo stesso turno
            let fine = ora - c.da_quando <= FINE_FRESCA_MS && (matches!(s0, Stato::Lavora | Stato::Aspetta) || (s0 == Stato::Finito && v.inizio_turno != t0));
            let tipo = match (c.stato, c.motivo) {
                (Stato::Aspetta, Motivo::Domanda) if nuova_domanda => Some((Tipo::Domanda, true)),
                (Stato::Aspetta, Motivo::Permesso) if !ignorata && nuovo_permesso => Some((Tipo::Permesso, prima_del_turno)),
                (Stato::Aspetta, Motivo::PermessoStimato) if !ignorata && nuovo_permesso && ora - (c.da_quando - SOGLIA_PERMESSO_MS) >= PERMESSO_STIMATO_MS => Some((Tipo::Permesso, prima_del_turno)),
                // ogni turno finito si riassume; si notifica solo se è durato più di 2 minuti
                (Stato::Finito, Motivo::FineTurno) if fine => Some((Tipo::Finito, v.inizio_turno.is_some_and(|t| c.da_quando - t > TURNO_LUNGO_MS))),
                (Stato::Finito, Motivo::ErroreApi) if fine => Some((Tipo::Errore, true)),
                _ => None,
            };
            if let Some((tipo, notifica)) = tipo {
                if tipo == Tipo::Permesso {
                    self.permesso_emesso.insert(k.clone(), attesa);
                    if notifica {
                        self.permesso_notificato.insert(k.clone(), turno);
                    }
                }
                eventi.push(Evento { chiave: k, tipo, progetto: v.progetto.clone(), motivo: c.azione.clone(), notifica });
            }
        }
        self.permesso_notificato.retain(|k, _| adesso.contains_key(k));
        self.permesso_emesso.retain(|k, _| adesso.contains_key(k));
        self.attesa_ignorata.retain(|k, _| adesso.contains_key(k));
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
            risposta_sensibile: false,
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
            su_cosa: None,
            esito: None,
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
        assert!(e[0].notifica);
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
    fn primo_giro_stimato_non_emette_dopo() {
        let mut m = Memoria::default();
        let t = 1_000_000;
        let stimato = vista("a", Stato::Aspetta, Motivo::PermessoStimato, t + 10_000, Some(t));
        assert!(m.passaggi(&[stimato.clone()], t + 20_000).is_empty());
        assert!(m.passaggi(&[stimato], t + 31_000).is_empty());
        // finita l'attesa, un nuovo permesso nello stesso turno torna a notificare
        m.passaggi(&[vista("a", Stato::Lavora, Motivo::Strumento, t, Some(t))], t + 32_000);
        let nuovo = vista("a", Stato::Aspetta, Motivo::Permesso, t + 33_000, Some(t));
        assert_eq!(m.passaggi(&[nuovo], t + 34_000).len(), 1);
    }

    #[test]
    fn finito_breve_emette_senza_notifica() {
        let mut m = Memoria::default();
        m.passaggi(&[lavora(0)], 1);
        let e = m.passaggi(&[vista("a", Stato::Finito, Motivo::FineTurno, 30_000, Some(0))], 31_000);
        assert_eq!(e.len(), 1);
        assert_eq!(e[0].tipo, Tipo::Finito);
        assert!(!e[0].notifica);
        // anche dopo un'attesa (toglie l'«aspetta:» vecchio) e senza inizio_turno
        let mut m = Memoria::default();
        m.passaggi(&[vista("a", Stato::Aspetta, Motivo::Domanda, 0, None)], 1);
        m.passaggi(&[vista("a", Stato::Aspetta, Motivo::Domanda, 0, None)], 2);
        let e = m.passaggi(&[vista("a", Stato::Finito, Motivo::FineTurno, 500_000, None)], 500_001);
        assert_eq!(e.len(), 1);
        assert!(!e[0].notifica);
    }

    #[test]
    fn finito_lungo_emette() {
        let mut m = Memoria::default();
        m.passaggi(&[lavora(0)], 1);
        let e = m.passaggi(&[vista("a", Stato::Finito, Motivo::FineTurno, 121_000, Some(0))], 122_000);
        assert_eq!(e.len(), 1);
        assert_eq!(e[0].tipo, Tipo::Finito);
        assert!(e[0].notifica);
    }

    #[test]
    fn errore_emette() {
        let mut m = Memoria::default();
        m.passaggi(&[lavora(0)], 1);
        let e = m.passaggi(&[vista("a", Stato::Finito, Motivo::ErroreApi, 10, Some(0))], 11);
        assert_eq!(e.len(), 1);
        assert_eq!(e[0].tipo, Tipo::Errore);
        assert!(e[0].notifica);
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
    fn finito_convertito_dopo_lungo_silenzio_non_emette() {
        // lo scanner porta a Finito un Inattivo con processo vivo: da_quando = ultima_attivita, vecchia di almeno 10 minuti
        let t = 10_000_000;
        for prima in [lavora(t), vista("a", Stato::Aspetta, Motivo::Domanda, t, Some(t))] {
            let mut m = Memoria::default();
            m.passaggi(&[prima.clone()], t + 1);
            m.passaggi(&[prima], t + 2);
            let convertito = vista("a", Stato::Finito, Motivo::FineTurno, t + 5, Some(t));
            assert!(m.passaggi(&[convertito], t + 600_005).is_empty());
            let errore = vista("a", Stato::Finito, Motivo::ErroreApi, t + 5, Some(t));
            let mut m = Memoria::default();
            m.passaggi(&[lavora(t)], t + 1);
            assert!(m.passaggi(&[errore], t + 600_005).is_empty());
        }
    }

    #[test]
    fn seconda_domanda_fra_due_letture_emette() {
        let mut m = Memoria::default();
        m.passaggi(&[lavora(0)], 1);
        assert_eq!(m.passaggi(&[vista("a", Stato::Aspetta, Motivo::Domanda, 5, Some(0))], 6).len(), 1);
        let e = m.passaggi(&[vista("a", Stato::Aspetta, Motivo::Domanda, 9, Some(0))], 10);
        assert_eq!(e.len(), 1);
        assert_eq!(e[0].tipo, Tipo::Domanda);
    }

    #[test]
    fn secondo_permesso_fra_due_letture_emette_se_turno_nuovo() {
        let mut m = Memoria::default();
        m.passaggi(&[lavora(0)], 1);
        assert_eq!(m.passaggi(&[vista("a", Stato::Aspetta, Motivo::Permesso, 5, Some(0))], 6).len(), 1);
        // stesso turno: evento (il riassunto si aggiorna) ma niente seconda notifica
        let e = m.passaggi(&[vista("a", Stato::Aspetta, Motivo::Permesso, 8, Some(0))], 9);
        assert_eq!(e.len(), 1);
        assert!(!e[0].notifica);
        let e = m.passaggi(&[vista("a", Stato::Aspetta, Motivo::Permesso, 20, Some(15))], 21);
        assert_eq!(e.len(), 1);
        assert_eq!(e[0].tipo, Tipo::Permesso);
    }

    #[test]
    fn secondo_turno_breve_fra_due_letture_emette() {
        let mut m = Memoria::default();
        m.passaggi(&[lavora(0)], 1);
        assert_eq!(m.passaggi(&[vista("a", Stato::Finito, Motivo::FineTurno, 10, Some(0))], 11).len(), 1);
        assert!(m.passaggi(&[vista("a", Stato::Finito, Motivo::FineTurno, 10, Some(0))], 12).is_empty());
        let e = m.passaggi(&[vista("a", Stato::Finito, Motivo::FineTurno, 30, Some(20))], 31);
        assert_eq!(e.len(), 1);
        assert_eq!(e[0].tipo, Tipo::Finito);
        assert!(!e[0].notifica);
    }

    #[test]
    fn due_fini_dello_stesso_turno_un_solo_evento() {
        // Claude scrive end_turn e poi system/turn_duration a pochi centesimi di secondo
        let mut m = Memoria::default();
        m.passaggi(&[lavora(0)], 1);
        let e = m.passaggi(&[vista("a", Stato::Finito, Motivo::FineTurno, 200_000, Some(0))], 200_001);
        assert_eq!(e.len(), 1);
        assert!(m.passaggi(&[vista("a", Stato::Finito, Motivo::FineTurno, 200_050, Some(0))], 200_051).is_empty());
    }

    #[test]
    fn errori_ripetuti_nello_stesso_turno_un_solo_evento() {
        // system/api_error a ogni tentativo
        let mut m = Memoria::default();
        m.passaggi(&[lavora(0)], 1);
        assert_eq!(m.passaggi(&[vista("a", Stato::Finito, Motivo::ErroreApi, 10, Some(0))], 11).len(), 1);
        assert!(m.passaggi(&[vista("a", Stato::Finito, Motivo::ErroreApi, 20, Some(0))], 21).is_empty());
        assert!(m.passaggi(&[vista("a", Stato::Finito, Motivo::ErroreApi, 30, Some(0))], 31).is_empty());
    }

    #[test]
    fn domanda_dall_hook_al_registro_non_si_ripete() {
        // con l'hook da_quando è il segnale S; dal registro è l'inizio dello strumento Q < S
        let mut m = Memoria::default();
        m.passaggi(&[lavora(0)], 1);
        assert_eq!(m.passaggi(&[vista("a", Stato::Aspetta, Motivo::Domanda, 500, Some(0))], 501).len(), 1);
        assert!(m.passaggi(&[vista("a", Stato::Aspetta, Motivo::Domanda, 400, Some(0))], 502).is_empty());
    }

    #[test]
    fn sessione_nuova_gia_in_attesa_emette() {
        let t = 1_000_000;
        let mut m = Memoria::default();
        m.passaggi(&[lavora(t)], t + 1);
        let e = m.passaggi(&[lavora(t), vista("b", Stato::Aspetta, Motivo::Domanda, t, Some(t))], t + 2);
        assert_eq!(e.len(), 1);
        assert_eq!((e[0].chiave.as_str(), e[0].tipo), ("claude:b", Tipo::Domanda));
        let e = m.passaggi(&[lavora(t), vista("c", Stato::Aspetta, Motivo::Permesso, t, Some(t))], t + 3);
        assert_eq!(e.len(), 1);
        assert_eq!(e[0].tipo, Tipo::Permesso);
        // permesso stimato: servono sempre i 30 secondi di strumento fermo
        let stimato = vista("d", Stato::Aspetta, Motivo::PermessoStimato, t + 10_000, Some(t));
        assert!(m.passaggi(&[lavora(t), stimato.clone()], t + 20_000).is_empty());
        assert_eq!(m.passaggi(&[lavora(t), stimato], t + 31_000).len(), 1);
    }

    #[test]
    fn attesa_ignorata_vale_solo_per_quell_attesa() {
        let mut m = Memoria::default();
        m.passaggi(&[vista("a", Stato::Aspetta, Motivo::Permesso, 5, Some(0))], 6);
        assert!(m.passaggi(&[vista("a", Stato::Aspetta, Motivo::Permesso, 5, Some(0))], 7).is_empty());
        // un'altra attesa, senza che nessuna lettura abbia visto la sessione uscire dal permesso
        let e = m.passaggi(&[vista("a", Stato::Aspetta, Motivo::Permesso, 50, Some(40))], 51);
        assert_eq!(e.len(), 1);
        assert_eq!(e[0].tipo, Tipo::Permesso);
    }

    #[test]
    fn permessi_successivi_nel_turno_emettono_senza_notifica() {
        let t = 1_000_000;
        let mut m = Memoria::default();
        m.passaggi(&[lavora(t)], t + 1);
        let e = m.passaggi(&[vista("a", Stato::Aspetta, Motivo::Permesso, t + 5, Some(t))], t + 6);
        assert!(e[0].notifica);
        m.passaggi(&[vista("a", Stato::Lavora, Motivo::Strumento, t, Some(t))], t + 7);
        let e = m.passaggi(&[vista("a", Stato::Aspetta, Motivo::Permesso, t + 9, Some(t))], t + 10);
        assert_eq!(e.len(), 1);
        assert!(!e[0].notifica);
        assert!(m.passaggi(&[vista("a", Stato::Aspetta, Motivo::Permesso, t + 9, Some(t))], t + 11).is_empty());
        // stimato nello stesso turno: dopo i 30 secondi un evento senza notifica, uno solo
        let stimato = vista("a", Stato::Aspetta, Motivo::PermessoStimato, t + 50_000, Some(t));
        let e = m.passaggi(&[stimato.clone()], t + 71_000);
        assert_eq!(e.len(), 1);
        assert!(!e[0].notifica);
        assert!(m.passaggi(&[stimato], t + 72_000).is_empty());
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
