//! Lettura incrementale dei registri: si legge solo la coda nuova di ogni file.
pub mod claude;
pub mod codex;
pub mod pi;

use crate::model::{Acc, Agente};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

pub struct Lettore {
    pub acc: Acc,
    offset: u64,
    resto: Vec<u8>,
}

impl Lettore {
    pub fn new(agente: Agente, file: PathBuf) -> Self {
        let id = file
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        Lettore { acc: Acc::new(agente, id, file), offset: 0, resto: Vec::new() }
    }

    /// Legge le righe aggiunte dall'ultima volta. Restituisce true se è cambiato qualcosa.
    pub fn aggiorna(&mut self) -> std::io::Result<bool> {
        let mut f = File::open(&self.acc.file)?;
        let len = f.metadata()?.len();
        if len < self.offset {
            // file riscritto o troncato: si ricomincia
            let (agente, file) = (self.acc.agente, self.acc.file.clone());
            *self = Lettore::new(agente, file);
        }
        if len == self.offset {
            return Ok(false);
        }
        f.seek(SeekFrom::Start(self.offset))?;
        let mut buf = Vec::with_capacity((len - self.offset) as usize);
        f.take(len - self.offset).read_to_end(&mut buf)?;
        self.offset = len;
        self.resto.extend_from_slice(&buf);
        let mut inizio = 0;
        for i in 0..self.resto.len() {
            if self.resto[i] == b'\n' {
                let riga = &self.resto[inizio..i];
                inizio = i + 1;
                if riga.is_empty() {
                    continue;
                }
                if let Ok(v) = serde_json::from_slice::<serde_json::Value>(riga) {
                    feed(&mut self.acc, &v);
                }
            }
        }
        self.resto.drain(..inizio);
        Ok(true)
    }
}

pub fn feed(acc: &mut Acc, v: &serde_json::Value) {
    match acc.agente {
        Agente::Claude => claude::feed(acc, v),
        Agente::Codex => codex::feed(acc, v),
        Agente::Pi => pi::feed(acc, v),
    }
}

/// Legge un intero file in un colpo (test e prima apertura).
pub fn leggi_tutto(agente: Agente, file: &Path) -> std::io::Result<Acc> {
    let mut l = Lettore::new(agente, file.to_path_buf());
    l.aggiorna()?;
    Ok(l.acc)
}
