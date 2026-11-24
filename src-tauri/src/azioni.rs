//! Azioni: portare in primo piano la scheda di una sessione viva, o riprenderne una chiusa.
use crate::localizza::Posizione;
use crate::model::Agente;
use std::process::Command;

/// Esito leggibile da mostrare all'utente.
pub type Esito = Result<String, String>;

fn osascript(script: &str) -> Result<String, String> {
    let out = Command::new("osascript").arg("-e").arg(script).output().map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

/// Testo sicuro dentro una stringa AppleScript.
fn as_str(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

fn attiva(app: &str) -> Esito {
    let st = Command::new("open").args(["-a", app]).status().map_err(|e| e.to_string())?;
    if st.success() { Ok(format!("{app} in primo piano")) } else { Err(format!("Non riesco ad aprire {app}")) }
}

/// Password del socket di cmux dal portachiavi. Letta una volta sola quando c'è (la Plancia chiede
/// i nomi degli spazi ogni 5 secondi). ponytail: se si cambia la password serve riaprire l'app.
fn password_cmux() -> Option<String> {
    static TROVATA: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    if let Some(p) = TROVATA.get() {
        return Some(p.clone());
    }
    let out = Command::new("security").args(["find-generic-password", "-s", "crew-deck", "-a", "cmux", "-w"]).output().ok()?;
    let p = out.status.success().then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())?;
    Some(TROVATA.get_or_init(|| p).clone())
}

/// Comando cmux che risponde in JSON (con la password del socket, se c'è).
pub fn cmux_json(args: &[&str]) -> Option<serde_json::Value> {
    let mut c = Command::new("cmux");
    c.args(args).env("CMUX_QUIET", "1");
    if let Some(p) = password_cmux() {
        c.env("CMUX_SOCKET_PASSWORD", p);
    }
    let out = c.output().ok()?;
    if !out.status.success() {
        return None;
    }
    serde_json::from_slice(&out.stdout).ok()
}

pub fn cmux(args: &[&str]) -> Result<(), String> {
    cmux_con(args, None)
}

/// Comando cmux con testo opzionale su stdin. Usa la password del socket dal portachiavi, se c'è.
pub fn cmux_con(args: &[&str], stdin: Option<&str>) -> Result<(), String> {
    let mut c = Command::new("cmux");
    c.args(args).env("CMUX_QUIET", "1");
    if let Some(p) = password_cmux() {
        c.env("CMUX_SOCKET_PASSWORD", p);
    }
    let out = if let Some(t) = stdin {
        use std::io::Write;
        c.stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped());
        let mut f = c.spawn().map_err(|e| e.to_string())?;
        f.stdin.take().unwrap().write_all(t.as_bytes()).map_err(|e| e.to_string())?;
        f.wait_with_output().map_err(|e| e.to_string())?
    } else {
        c.output().map_err(|e| e.to_string())?
    };
    let err = String::from_utf8_lossy(&out.stderr).to_string() + &String::from_utf8_lossy(&out.stdout);
    if out.status.success() && !err.contains("ERROR") { Ok(()) } else { Err(err.trim().to_string()) }
}

/// Porta in primo piano la scheda esatta della sessione.
pub fn vai(p: &Posizione) -> Esito {
    match p.app.as_str() {
        "cmux" => {
            let _ = attiva("cmux");
            if let (Some(ws), Some(sf)) = (&p.cmux_workspace, &p.cmux_surface) {
                match cmux(&["select-workspace", "--workspace", ws]).and_then(|_| cmux(&["focus-panel", "--panel", sf, "--workspace", ws])) {
                    Ok(()) => return Ok("Aperta in cmux".into()),
                    Err(e) if e.contains("negato") || e.to_lowercase().contains("denied") || e.to_lowercase().contains("password") => {
                        return Ok("cmux è in primo piano. Per aprire la scheda esatta: in cmux, Impostazioni › Automazione, scegli la modalità password e salvala in Crew Deck.".into())
                    }
                    Err(e) => return Err(e),
                }
            }
            Ok("cmux in primo piano".into())
        }
        "Ghostty" => {
            let Some(tty) = &p.tty else { return attiva("Ghostty") };
            let script = format!(
                "tell application \"Ghostty\"\n repeat with w in windows\n repeat with t in tabs of w\n repeat with term in terminals of t\n if tty of term is {} then\n focus term\n activate\n return \"ok\"\n end if\n end repeat\n end repeat\n end repeat\n activate\n return \"non trovata\"\nend tell",
                as_str(tty)
            );
            match osascript(&script)?.as_str() {
                "ok" => Ok("Aperta in Ghostty".into()),
                _ => Ok("Ghostty in primo piano (scheda non trovata)".into()),
            }
        }
        "Terminal" => {
            let Some(tty) = &p.tty else { return attiva("Terminal") };
            let script = format!(
                "tell application \"Terminal\"\n repeat with w in windows\n repeat with t in tabs of w\n if tty of t is {} then\n set selected of t to true\n set index of w to 1\n activate\n return \"ok\"\n end if\n end repeat\n end repeat\n activate\nend tell",
                as_str(tty)
            );
            osascript(&script).map(|_| "Aperta in Terminale".into())
        }
        "iTerm" | "iTerm2" => {
            let Some(tty) = &p.tty else { return attiva("iTerm") };
            let script = format!(
                "tell application \"iTerm2\"\n repeat with w in windows\n repeat with t in tabs of w\n repeat with s in sessions of t\n if tty of s is {} then\n select w\n select t\n select s\n activate\n return \"ok\"\n end if\n end repeat\n end repeat\n end repeat\n activate\nend tell",
                as_str(tty)
            );
            osascript(&script).map(|_| "Aperta in iTerm".into())
        }
        altra => attiva(altra),
    }
}

/// Comando per riprendere una sessione chiusa.
pub fn comando_ripresa(agente: Agente, id: &str, file: &str) -> String {
    match agente {
        Agente::Claude => format!("claude --resume {id}"),
        Agente::Codex => format!("codex resume {id}"),
        Agente::Pi => format!("pi --session '{}'", file.replace('\'', "'\\''")),
    }
}

/// Apre una scheda nuova nella cartella della sessione, con il comando di ripresa già scritto e lanciato.
pub fn riprendi(agente: Agente, id: &str, file: &str, cwd: &str, terminale: &str) -> Esito {
    let cmd = comando_ripresa(agente, id, file);
    match terminale {
        "cmux" => {
            match cmux(&["new-workspace", "--cwd", cwd, "--command", &cmd, "--focus", "true"]) {
                Ok(()) => { let _ = attiva("cmux"); Ok("Ripresa in cmux".into()) }
                Err(_) => riprendi(agente, id, file, cwd, "Ghostty").map(|m| format!("{m} (cmux non accetta comandi da fuori: vedi Impostazioni › Automazione)")),
            }
        }
        "Terminal" => {
            let script = format!("tell application \"Terminal\"\n do script {}\n activate\nend tell", as_str(&format!("cd {} && {}", shell_q(cwd), cmd)));
            osascript(&script).map(|_| "Ripresa in Terminale".into())
        }
        _ => {
            let script = format!(
                "tell application \"Ghostty\"\n set cfg to new surface configuration\n set initial working directory of cfg to {}\n set initial input of cfg to {}\n if (count of windows) > 0 then\n new tab in front window with configuration cfg\n else\n new window with configuration cfg\n end if\n activate\nend tell",
                as_str(cwd),
                as_str(&format!("{cmd}\n"))
            );
            osascript(&script).map(|_| "Ripresa in Ghostty".into())
        }
    }
}

fn shell_q(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// Indirizzo per aprire una sessione nata in un'app desktop, sulla conversazione giusta.
pub fn link_desktop(agente: Agente, id: &str, origine: Option<&str>) -> Option<(String, &'static str)> {
    let o = origine.unwrap_or("");
    let id_ok = !id.is_empty() && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    if !id_ok {
        return None;
    }
    match agente {
        Agente::Claude if o == "claude-desktop" => Some((format!("claude://resume?session={id}"), "app Claude")),
        Agente::Codex if o.starts_with("Codex Desktop") => Some((format!("codex://threads/{id}"), "app Codex")),
        _ => None,
    }
}

pub fn apri_link(url: &str, nome: &str) -> Esito {
    let st = Command::new("open").arg(url).status().map_err(|e| e.to_string())?;
    if st.success() { Ok(format!("Aperta nell'{nome}")) } else { Err(format!("Non riesco ad aprire l'{nome}")) }
}

/// cmux accetta comandi da Crew Deck? (installato, e socket aperto in modalità password o simile)
pub fn stato_cmux() -> serde_json::Value {
    let installato = Command::new("cmux").arg("--help").output().map(|o| o.status.success()).unwrap_or(false);
    let accesso = installato && cmux(&["workspace", "list"]).is_ok();
    serde_json::json!({ "installato": installato, "accesso": accesso, "password_salvata": password_cmux().is_some() })
}
