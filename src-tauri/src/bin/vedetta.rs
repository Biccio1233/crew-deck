//! Prova da terminale: elenca le sessioni con lo stato, senza interfaccia.
use crew_deck_lib::{ora_ms, scanner::Scanner};

fn main() {
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
