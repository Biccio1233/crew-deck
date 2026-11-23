//! Collegamento processo ↔ sessione, senza processi veri.
use crew_deck_lib::localizza::{agente_di, app_di, collega, Cercata, Processo};
use crew_deck_lib::model::Agente;
use std::collections::HashMap;

fn pr(pid: u32, ppid: u32, tty: Option<&str>, cmd: &str) -> Processo {
    Processo { pid, ppid, tty: tty.map(|t| t.to_string()), comando: cmd.into() }
}

#[test]
fn riconosce_agenti_e_app() {
    assert_eq!(agente_di("/Users/x/.local/bin/claude"), Some(Agente::Claude));
    assert_eq!(agente_di("pi"), Some(Agente::Pi));
    assert_eq!(agente_di("/Users/x/.codex/packages/app-server-daemon/releases/1/bin/codex"), None);
    assert_eq!(app_di("/Applications/cmux.app/Contents/MacOS/cmux").as_deref(), Some("cmux"));
    assert_eq!(app_di("/usr/bin/login"), None);
}

#[test]
fn collega_cmux_esatto_e_ghostty_per_cartella() {
    let mut t = HashMap::new();
    for p in [
        pr(658, 1, None, "/Applications/cmux.app/Contents/MacOS/cmux"),
        pr(700, 658, Some("/dev/ttys001"), "/usr/bin/login"),
        pr(701, 700, Some("/dev/ttys001"), "-/bin/zsh"),
        pr(702, 701, Some("/dev/ttys001"), "pi"),
        pr(900, 1, None, "/Applications/Ghostty.app/Contents/MacOS/ghostty"),
        pr(901, 900, Some("/dev/ttys006"), "/usr/bin/login"),
        pr(902, 901, Some("/dev/ttys006"), "/Users/x/.local/bin/claude"),
    ] {
        t.insert(p.pid, p);
    }
    let agenti: Vec<(&Processo, Agente)> = vec![(&t[&702], Agente::Pi), (&t[&902], Agente::Claude)];
    let cwd: HashMap<u32, String> = [(702, "/p/a".to_string()), (902, "/p/b".to_string())].into();
    let cmux: HashMap<String, (u32, String, String)> = [("pi-1".to_string(), (702, "WS".to_string(), "SF".to_string()))].into();
    let sess = [
        Cercata { chiave: "pi:pi-1".into(), id: "pi-1", agente: Agente::Pi, cwd: Some("/p/a"), ultima_attivita: 10 },
        Cercata { chiave: "claude:c-1".into(), id: "c-1", agente: Agente::Claude, cwd: Some("/p/b"), ultima_attivita: 5 },
        Cercata { chiave: "claude:c-0".into(), id: "c-0", agente: Agente::Claude, cwd: Some("/p/b"), ultima_attivita: 1 },
    ];
    let m = collega(&sess, &t, &agenti, &cwd, &cmux);
    let a = &m["pi:pi-1"];
    assert_eq!((a.app.as_str(), a.cmux_surface.as_deref(), a.incerta), ("cmux", Some("SF"), false));
    let b = &m["claude:c-1"];
    assert_eq!((b.app.as_str(), b.tty.as_deref()), ("Ghostty", Some("/dev/ttys006")));
    // la sessione più vecchia nella stessa cartella non prende lo stesso processo
    assert!(!m.contains_key("claude:c-0"));
}
