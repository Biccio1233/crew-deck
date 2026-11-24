//! Dove gira ogni sessione: processo vivo, app che lo ospita, scheda del terminale.
//! Oggi solo macOS (ps, lsof, AppleScript); su altri sistemi restituisce mappe vuote.
use crate::model::Agente;
use serde::Serialize;
use std::collections::HashMap;
use std::process::Command;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Posizione {
    /// Nome dell'app che ospita il processo: cmux, Ghostty, Terminal, iTerm2, Code, Claude…
    pub app: String,
    pub pid: u32,
    pub tty: Option<String>,
    pub cwd: Option<String>,
    /// Spazio di lavoro e pannello di cmux, quando la sessione è dentro cmux.
    pub cmux_workspace: Option<String>,
    pub cmux_surface: Option<String>,
    /// Nome dello spazio di lavoro come appare nella colonna di cmux.
    pub cmux_titolo: Option<String>,
    /// true se il collegamento processo↔sessione è dedotto da agente e cartella.
    pub incerta: bool,
}

#[derive(Debug, Clone)]
pub struct Processo {
    pub pid: u32,
    pub ppid: u32,
    pub tty: Option<String>,
    pub comando: String,
}

/// Che agente è un processo, dal suo eseguibile. Esclude i demoni delle app desktop.
pub fn agente_di(comando: &str) -> Option<Agente> {
    if comando.contains("app-server-daemon") || comando.contains(".app/Contents/") {
        return None;
    }
    match comando.rsplit('/').next().unwrap_or(comando) {
        "claude" => Some(Agente::Claude),
        "codex" => Some(Agente::Codex),
        "pi" => Some(Agente::Pi),
        _ => None,
    }
}

/// Nome dell'app da un percorso come `/Applications/cmux.app/Contents/MacOS/cmux`.
pub fn app_di(comando: &str) -> Option<String> {
    let i = comando.find(".app/Contents/MacOS/")?;
    let prima = &comando[..i];
    Some(prima.rsplit('/').next().unwrap_or(prima).to_string())
}

/// Risale i genitori fino alla prima app.
pub fn app_ospite(pid: u32, tabella: &HashMap<u32, Processo>) -> Option<String> {
    let mut p = pid;
    for _ in 0..12 {
        let pr = tabella.get(&p)?;
        if let Some(a) = app_di(&pr.comando) {
            return Some(a);
        }
        if pr.ppid <= 1 {
            return None;
        }
        p = pr.ppid;
    }
    None
}

fn tabella_processi() -> HashMap<u32, Processo> {
    let mut t = HashMap::new();
    let Ok(out) = Command::new("ps").args(["-axo", "pid=,ppid=,tty=,comm="]).output() else { return t };
    for riga in String::from_utf8_lossy(&out.stdout).lines() {
        let mut it = riga.split_whitespace();
        let (Some(pid), Some(ppid), Some(tty)) = (it.next(), it.next(), it.next()) else { continue };
        let comando = it.collect::<Vec<_>>().join(" ");
        let (Ok(pid), Ok(ppid)) = (pid.parse(), ppid.parse()) else { continue };
        let tty = if tty == "??" { None } else { Some(format!("/dev/{tty}")) };
        t.insert(pid, Processo { pid, ppid, tty, comando });
    }
    t
}

fn cartelle(pids: &[u32]) -> HashMap<u32, String> {
    let mut m = HashMap::new();
    if pids.is_empty() {
        return m;
    }
    let lista = pids.iter().map(|p| p.to_string()).collect::<Vec<_>>().join(",");
    let Ok(out) = Command::new("lsof").args(["-a", "-d", "cwd", "-p", &lista, "-Fpn"]).output() else { return m };
    let mut pid = 0u32;
    for riga in String::from_utf8_lossy(&out.stdout).lines() {
        if let Some(p) = riga.strip_prefix('p') {
            pid = p.parse().unwrap_or(0);
        } else if let Some(n) = riga.strip_prefix('n') {
            m.insert(pid, n.to_string());
        }
    }
    m
}

/// Registro delle sessioni di cmux: session_id → (pid, workspace, surface). Non serve il socket.
fn registro_cmux() -> HashMap<String, (u32, String, String)> {
    let mut m = HashMap::new();
    let Ok(out) = Command::new("cmux").args(["sessions", "list", "--json"]).output() else { return m };
    let Ok(v) = serde_json::from_slice::<serde_json::Value>(&out.stdout) else { return m };
    let lista = v.get("sessions").and_then(|s| s.as_array()).cloned().unwrap_or_default();
    for s in lista {
        let id = s.get("session_id").and_then(|x| x.as_str());
        let pid = s.get("pid").and_then(|x| x.as_u64().or_else(|| x.as_str().and_then(|t| t.parse().ok())));
        let ws = s.get("workspace_id").and_then(|x| x.as_str());
        let sf = s.get("surface_id").and_then(|x| x.as_str());
        if let (Some(id), Some(pid), Some(ws), Some(sf)) = (id, pid, ws, sf) {
            m.insert(id.to_string(), (pid as u32, ws.to_string(), sf.to_string()));
        }
    }
    m
}

/// Una sessione da collegare: chiave, agente, cartella, ultima attività.
pub struct Cercata<'a> {
    pub chiave: String,
    pub id: &'a str,
    pub agente: Agente,
    pub cwd: Option<&'a str>,
    pub ultima_attivita: i64,
}

/// Collega le sessioni ai processi vivi. Restituisce chiave → posizione.
pub fn localizza(sessioni: &[Cercata]) -> HashMap<String, Posizione> {
    if !cfg!(target_os = "macos") {
        return HashMap::new();
    }
    let tabella = tabella_processi();
    let agenti: Vec<(&Processo, Agente)> = tabella.values().filter_map(|p| agente_di(&p.comando).map(|a| (p, a))).collect();
    let cwd = cartelle(&agenti.iter().map(|(p, _)| p.pid).collect::<Vec<_>>());
    let cmux = registro_cmux();
    let mut out = collega(sessioni, &tabella, &agenti, &cwd, &cmux);
    if out.values().any(|p| p.cmux_workspace.is_some()) {
        let titoli = crate::azioni::cmux_json(&["workspace", "list", "--json"]).map(|v| titoli_cmux(&v)).unwrap_or_default();
        for p in out.values_mut() {
            p.cmux_titolo = p.cmux_workspace.as_ref().and_then(|w| titoli.get(w).cloned());
        }
    }
    out
}

/// Spazio di lavoro → nome mostrato nella colonna di cmux, da `cmux workspace list --json`.
/// ponytail: legge solo la finestra corrente di cmux; con più finestre gli spazi delle altre restano
/// senza nome (si mostra la cartella). Si supera chiedendo la lista per ogni finestra.
pub fn titoli_cmux(v: &serde_json::Value) -> HashMap<String, String> {
    v.get("workspaces")
        .and_then(|w| w.as_array())
        .into_iter()
        .flatten()
        .filter_map(|w| {
            let id = w.get("id")?.as_str()?;
            let t = w.get("title")?.as_str()?.trim();
            (!t.is_empty()).then(|| (id.to_string(), t.to_string()))
        })
        .collect()
}

/// Parte pura del collegamento, testabile senza processi veri.
pub fn collega(
    sessioni: &[Cercata],
    tabella: &HashMap<u32, Processo>,
    agenti: &[(&Processo, Agente)],
    cwd: &HashMap<u32, String>,
    cmux: &HashMap<String, (u32, String, String)>,
) -> HashMap<String, Posizione> {
    let mut out = HashMap::new();
    let mut usati: Vec<u32> = Vec::new();
    let vivo = |pid: u32| agenti.iter().any(|(p, _)| p.pid == pid);
    let pos = |p: &Processo, incerta: bool, cm: Option<(&String, &String)>| Posizione {
        app: app_ospite(p.pid, tabella).unwrap_or_else(|| "Terminale".into()),
        pid: p.pid,
        tty: p.tty.clone(),
        cwd: cwd.get(&p.pid).cloned(),
        cmux_workspace: cm.map(|c| c.0.clone()),
        cmux_surface: cm.map(|c| c.1.clone()),
        cmux_titolo: None,
        incerta,
    };

    // 1. registro di cmux: collegamento esatto
    for s in sessioni {
        if let Some((pid, ws, sf)) = cmux.get(s.id) {
            if vivo(*pid) {
                if let Some(p) = tabella.get(pid) {
                    out.insert(s.chiave.clone(), pos(p, false, Some((ws, sf))));
                    usati.push(*pid);
                }
            }
        }
    }
    // 2. gli altri: stesso agente e stessa cartella; la sessione più recente prende il processo più recente
    let mut restanti: Vec<&Cercata> = sessioni.iter().filter(|s| !out.contains_key(&s.chiave)).collect();
    restanti.sort_by(|a, b| b.ultima_attivita.cmp(&a.ultima_attivita));
    for s in restanti {
        let Some(dir) = s.cwd else { continue };
        let mut candidati: Vec<&Processo> = agenti
            .iter()
            .filter(|(p, a)| *a == s.agente && !usati.contains(&p.pid) && cwd.get(&p.pid).map(|c| c == dir).unwrap_or(false))
            .map(|(p, _)| *p)
            .collect();
        candidati.sort_by(|a, b| b.pid.cmp(&a.pid));
        if let Some(p) = candidati.first() {
            let incerta = candidati.len() > 1;
            out.insert(s.chiave.clone(), pos(p, incerta, None));
            usati.push(p.pid);
        }
    }
    out
}
