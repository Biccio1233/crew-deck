//! Riservatezza: cosa non parte mai (password e credenziali) e cosa parte solo oscurato.
//! Le regole stanno in `giudice/giudice.py`; qui le usano riassunti, carteggio e notifiche.
use crate::carteggio::{con_tempo, GIUDICE};
use std::process::Command;

/// Il testo con i `FILTRI` di giudice.py applicati (email, chiavi, IBAN, codici fiscali, telefoni, percorsi, link).
/// Errore se python3 manca o fallisce: chi chiama non invia niente.
pub fn filtra(testo: &str) -> Result<String, String> {
    let mut cmd = Command::new("python3");
    cmd.args(["-I", "-c", GIUDICE, "--filtra"]);
    con_tempo(cmd, Some(testo), 10)
}

/// Porta a mano la regex `SENSIBILE` di `giudice/giudice.py` (stessi termini, senza maiuscole, `\b` per pwd, pin, puk, otp)
/// per non aggiungere la dipendenza `regex`. Se cambia là, va cambiata qui (lo ricorda il test `filo_sensibile`).
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

    /// Filo d'allarme: `sensibile` è la copia a mano di questa riga di giudice.py. Se il test fallisce,
    /// la regex è cambiata là: aggiorna `sensibile` (e i suoi casi) e poi questa costante.
    #[test]
    fn filo_sensibile() {
        const PORTATA: &str = r#"SENSIBILE = re.compile(r"(passw|\bpwd\b|password|credenzial|\bpin\b|\bpuk\b|seed phrase|frase segreta|codice (di accesso|segreto)|chiave privata|private key|\botp\b)", re.I)"#;
        let riga = include_str!("../../giudice/giudice.py").lines().find(|r| r.starts_with("SENSIBILE = re.compile(")).expect("riga SENSIBILE non trovata in giudice.py");
        assert_eq!(riga, PORTATA);
    }

    #[test]
    fn filtra_oscura_percorsi_chiavi_email() {
        if std::process::Command::new("python3").arg("--version").output().is_err() {
            eprintln!("python3 non c'è: prova di filtra saltata");
            return;
        }
        let f = filtra("apri /Users/mario/x/file.rs con sk-abcdefghijklmnop1234 e scrivi a mario.rossi@esempio.it — fatto").unwrap();
        assert_eq!(f, "apri <percorso>/file.rs con <chiave> e scrivi a <email> — fatto");
    }
}
