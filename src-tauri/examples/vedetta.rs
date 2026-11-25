//! Prova da terminale: elenca le sessioni con lo stato, senza interfaccia.
use crew_deck_lib::{ora_ms, scanner::Scanner};

fn main() {
    crew_deck_lib::completa_path();
    let home = dirs::home_dir().expect("home");
    let mut s = Scanner::new(&home);
    let t0 = std::time::Instant::now();
    s.aggiorna();
    let prima = t0.elapsed();
    let t1 = std::time::Instant::now();
    s.aggiorna();
    let seconda = t1.elapsed();
    let tutte = std::env::args().any(|a| a == "--tutte");
    let viste: Vec<_> = s.viste(ora_ms()).into_iter().filter(|v| tutte || !v.automatica).collect();
    let args: Vec<String> = std::env::args().collect();
    if let Some(i) = args.iter().position(|a| a == "--suggerisci") {
        let ag = match args[i + 1].as_str() { "claude" => crew_deck_lib::model::Agente::Claude, "codex" => crew_deck_lib::model::Agente::Codex, _ => crew_deck_lib::model::Agente::Pi };
        let (rq, rs) = s.conversazione(ag, &args[i + 2]).expect("sessione");
        println!("{}", serde_json::to_string_pretty(&crew_deck_lib::carteggio::suggerisci(rq.as_deref(), rs.as_deref().unwrap_or("")).unwrap_or_else(|e| serde_json::json!({"errore": e}))).unwrap());
        return;
    }
    if let Some(i) = args.iter().position(|a| a == "--riscrivi") {
        // prova: --riscrivi <rifinisci|approfondisci> "testo"
        println!("{}", serde_json::to_string_pretty(&crew_deck_lib::carteggio::riscrivi(&args[i + 1], &args[i + 2], None, &[]).unwrap_or_else(|e| serde_json::json!({"errore": e}))).unwrap());
        return;
    }
    if let Some(i) = args.iter().position(|a| a == "--apri") {
        // prova: --apri <agente> <id>  (solo sessioni vive: porta in primo piano la scheda)
        let ag = match args[i + 1].as_str() { "claude" => crew_deck_lib::model::Agente::Claude, "codex" => crew_deck_lib::model::Agente::Codex, _ => crew_deck_lib::model::Agente::Pi };
        match s.per_azione(ag, &args[i + 2]) {
            Some((Some(p), ..)) => println!("{:?}", crew_deck_lib::azioni::vai(&p)),
            Some((None, ..)) => println!("sessione chiusa: nessuna scheda da aprire"),
            None => println!("sessione non trovata"),
        }
        return;
    }
    if let Some(i) = args.iter().position(|a| a == "--riprendi") {
        // prova: --riprendi <agente> <id> [terminale]  (riapre una sessione chiusa in una scheda nuova)
        let ag = match args[i + 1].as_str() { "claude" => crew_deck_lib::model::Agente::Claude, "codex" => crew_deck_lib::model::Agente::Codex, _ => crew_deck_lib::model::Agente::Pi };
        match s.per_azione(ag, &args[i + 2]) {
            Some((_, file, Some(cwd), ..)) => println!("{:?}", crew_deck_lib::azioni::riprendi(ag, &args[i + 2], &file, &cwd, args.get(i + 3).map(|s| s.as_str()).unwrap_or("Ghostty"))),
            _ => println!("sessione non trovata o senza cartella"),
        }
        return;
    }
    if std::env::args().any(|a| a == "--json") {
        println!("{}", serde_json::to_string(&s.viste(ora_ms())).unwrap());
        return;
    }
    let n = std::env::args().nth(1).and_then(|x| x.parse().ok()).unwrap_or(15);
    for v in viste.iter().take(n) {
        let icona = match v.calcolo.stato {
            crew_deck_lib::engine::Stato::Aspetta => "🚩",
            crew_deck_lib::engine::Stato::Lavora => "⛵",
            crew_deck_lib::engine::Stato::Finito => "🏁",
            crew_deck_lib::engine::Stato::Inattivo => "⚓",
        };
        let tok = v.token.input + v.token.output + v.token.cache_lettura + v.token.cache_scrittura;
        println!(
            "{icona} {:<6} {:<22} {:<10} {:>9} tok {:>8}  {}",
            format!("{:?}", v.agente).to_lowercase(),
            v.progetto.chars().take(22).collect::<String>(),
            format!("{:?}", v.ruolo).to_lowercase(),
            tok,
            v.costo_usd.map(|c| format!("${c:.2}")).unwrap_or_default(),
            v.calcolo.azione.chars().take(60).collect::<String>()
        );
    }
    eprintln!("{} sessioni · prima lettura {:?} · rilettura {:?}", viste.len(), prima, seconda);
}
