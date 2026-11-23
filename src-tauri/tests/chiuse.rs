//! Chiudere una sessione nella vista: resta chiusa finché non torna attiva.
use crew_deck_lib::model::Agente;
use crew_deck_lib::scanner::Scanner;
use std::io::Write;
use std::time::Duration;

fn riga(sec: i64, testo: &str) -> String {
    let ts = chrono::DateTime::from_timestamp(1_800_000_000 + sec, 0).unwrap().to_rfc3339();
    format!("{{\"type\":\"user\",\"timestamp\":\"{ts}\",\"cwd\":\"/p/x\",\"entrypoint\":\"cli\",\"message\":{{\"content\":\"{testo}\"}}}}\n")
}

#[test]
fn chiusa_finche_non_torna_attiva() {
    let dir = std::env::temp_dir().join(format!("crewdeck-test-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("progetto")).unwrap();
    let file = dir.join("progetto/sess-1.jsonl");
    std::fs::write(&file, riga(0, "prima richiesta")).unwrap();
    let mut s = Scanner::con_radici(vec![(Agente::Claude, dir.clone())], Duration::from_secs(1_000_000_000));
    s.aggiorna();
    let ora = 1_800_000_100_000;
    assert!(!s.viste(ora)[0].chiusa);
    assert!(s.chiudi(Agente::Claude, "sess-1"));
    assert!(s.viste(ora)[0].chiusa, "dopo la chiusura è nascosta");
    // la sessione riprende altrove: attività nuova dopo la chiusura
    std::fs::OpenOptions::new().append(true).open(&file).unwrap().write_all(riga(50, "ripresa").as_bytes()).unwrap();
    s.aggiorna();
    assert!(!s.viste(ora)[0].chiusa, "con attività nuova ricompare");
    // e si può chiudere e ripristinare a mano
    s.chiudi(Agente::Claude, "sess-1");
    s.ripristina(Agente::Claude, "sess-1");
    assert!(!s.viste(ora)[0].chiusa);
    let _ = std::fs::remove_dir_all(dir);
}
