# Nuova sessione: cartella e cervello centrale — piano

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** In Nuova sessione scegliere/creare la cartella di lavoro e collegare un cervello centrale facoltativo, con spiegazione e procedura guidata fatta dall'agente.

**Architecture:** I testi (riga del cervello, richiesta di creazione + metodo) si compongono in Rust (`carteggio.rs`) per poterli provare; la finestra di scelta usa `osascript choose folder`; l'interfaccia sta in `Carteggio.svelte` e ricorda il cervello in localStorage.

**Tech Stack:** Tauri 2 (Rust), Svelte 5, AppleScript.

**Spec:** `docs/superpowers/specs/2026-11-25-nuova-sessione-cartella-cervello-design.md`

## Global Constraints
- Nessuna dipendenza nuova.
- Crew Deck non scrive file nel cervello né nei progetti.
- Testo della riga, esatto: «Cervello centrale: <percorso>. Prima di cominciare leggi le sue istruzioni. Se questa cartella non ha un cervello locale, chiedi al cervello centrale come si crea e crealo. A fine lavoro annota cosa hai fatto dove il cervello centrale indica.»
- Interfaccia in italiano, percorsi mostrati con `~` al posto della home.

## Review Focus
- Annulla nella finestra di scelta → nessun cambiamento, nessun errore mostrato.
- Percorso con virgolette, apostrofi o spazi (es. `Mario Rossi/l'archivio "note"`) → riga e cartella corrette (AppleScript con `as_str`, prompt via file temporaneo).
- Cervello collegato ma casella tolta → il messaggio parte senza riga.
- Cartella del cervello cancellata dopo il collegamento → la riga parte lo stesso; l'agente se ne accorge (nessun controllo in Crew Deck, scelta voluta).
- Invio a una sessione già aperta → mai la riga.

---

### Task 1: testi e comandi in Rust

**Files:**
- Modify: `src-tauri/src/carteggio.rs` (dopo `nuova_sessione`)
- Create: `giudice/metodo-cervello.md` (metodo dalla sezione «Dentro un cervello» di `studio di metodo dell'autore (non nel repo)`, ridotto, prima versione)
- Modify: `src-tauri/src/lib.rs` (comandi + invoke_handler)

**Interfaces — Produces:**
- `pub fn riga_cervello(percorso: &str) -> String` — il testo esatto dei vincoli globali, seguito da `\n\n`.
- `pub fn richiesta_crea_cervello() -> String` — richiesta pronta + `include_str!("../../giudice/metodo-cervello.md")`.
- `pub fn scegli_cartella(titolo: &str) -> Result<Option<String>, String>` — `choose folder with prompt`; errore -128 (Annulla) → `Ok(None)`; percorso POSIX senza `/` finale.
- Comandi Tauri: `scegli_cartella(titolo: String) -> Result<Option<String>, String>`, `riga_cervello(percorso: String) -> String`, `crea_cervello(agente: String, cartella: String) -> Result<String, String>` (= `nuova_sessione(agente, cartella, &richiesta_crea_cervello())`).

- [ ] Test `riga_cervello_testo_esatto`: `riga_cervello("/a/b c")` inizia con «Cervello centrale: /a/b c. Prima di cominciare» e finisce con «dove il cervello centrale indica.\n\n».
- [ ] Test `richiesta_crea_cervello_contiene_metodo`: contiene «spiegami ogni passo» e «AGENTS.md».
- [ ] `cargo test` → falliscono; scrivere il codice; `cargo test` → tutti verdi.
- [ ] Commit.

### Task 2: interfaccia in Carteggio

**Files:** Modify `src/lib/Carteggio.svelte`.

**Interfaces — Consumes:** i tre comandi del Task 1.

- [ ] «Scegli…» (classe `sec`) accanto al menu cartelle: `scegli_cartella("Cartella di lavoro")`; se torna un percorso, lo aggiunge in testa a una lista `cartelleScelte` (stato) unita a `cartelle`, e lo seleziona.
- [ ] Riga «Cervello centrale ?» sotto le scelte, solo con `destinazione === "nuova"`: se collegato mostra percorso con `~`, «Cambia», «Togli» e la casella «Aggiungi in testa al messaggio» (accesa) con l'anteprima della riga; se no «Collega…». Persistenza `localStorage["crewdeck.cervello"]`.
- [ ] «?» apre un riquadro con la spiegazione (3-4 frasi: cos'è, perché serve agli agenti, cosa fa Crew Deck: solo una riga in testa al messaggio, non scrive file) e il pulsante «Creane uno»: `scegli_cartella("Dove creare il cervello")` → `crea_cervello(agenteNuovo, cartella)`, esito nel toast/riga di stato già usata da `invia`.
- [ ] In `invia`, solo per `nuova`: `testo` inviato = `riga_cervello(cervello) + testo` se cervello e casella; la conferma aggiunge «, con la riga del cervello centrale» quando c'è.
- [ ] `npm run check` senza errori; prova nell'anteprima del browser e dall'app (Review Focus 1, 3, 5).
- [ ] Commit, differential-review, pacchetto universale.
