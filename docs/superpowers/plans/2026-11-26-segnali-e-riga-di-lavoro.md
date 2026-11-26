# Segnali e riga «su cosa lavora» — piano di realizzazione

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** notifiche di macOS quando una sessione aspetta l'autore o finisce un lavoro lungo, e due righe per sessione («su cosa» ed «esito o domanda») scritte da Haiku con riassunti incrementali.

**Architecture:** il ciclo della vedetta in `lib.rs` (ogni 2 s) passa le viste a un rilevatore puro di passaggi di stato (`segnali.rs`) che produce eventi; gli eventi diventano notifiche (`tauri-plugin-notification`) e richieste in coda a un unico thread dei riassunti (`riassunti.rs`), che chiama `claude -p --model haiku` e salva il risultato in `~/.crew-deck/riassunti/`. Le viste emesse all'interfaccia portano i campi `su_cosa` ed `esito` letti dai riassunti salvati.

**Tech Stack:** Rust (Tauri 2), Svelte 5, `tauri-plugin-notification` 2, `claude` CLI.

**Spec:** `docs/superpowers/specs/2026-11-26-segnali-e-riga-di-lavoro-design.md`

## Global Constraints
- Una sola dipendenza nuova: `tauri-plugin-notification` (Rust) e `@tauri-apps/plugin-notification` (JS) solo se serve dal lato interfaccia; nient'altro.
- Soglie: «ha finito» notifica solo se il turno è durato **più di 2 minuti** (120 000 ms); permesso stimato notifica solo dopo **30 secondi** di fermo dall'inizio dello strumento; notifica entro **3 secondi** dal passaggio; riassunto entro **20 secondi**.
- «su cosa» al massimo **8 parole**; «esito o domanda» al massimo **12 parole**; in attesa inizia con «aspetta: ».
- Nessuna notifica e nessun riassunto per sessioni `automatica` o `chiusa`; nessun evento al primo giro dopo l'avvio.
- `claude -p` sempre con `--tools "" --no-session-persistence --setting-sources "" --output-format text`, `CREWDECK_GIUDICE=0`, cartella temporanea, come in `carteggio.rs`.
- Nessun testo che corrisponde al filtro password (`SENSIBILE` di `giudice/giudice.py`) esce dal Mac.
- Riassunti spenti finché l'autore non risponde sì alla domanda del primo avvio (risposta predefinita no).
- Repo pubblico: niente percorsi personali, nomi o dati veri nei file versionati.
- Ogni commit termina con `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`. Prima del merge: `differential-review` e secondo parere `codex review`.

## Review Focus
- Sessione che passa da «lavora» a «aspetta» e torna a «lavora» più volte nello stesso turno (permessi a raffica): una sola notifica di permesso per turno.
- App aperta con sessioni già in «aspetta» o «ha finito»: nessuna notifica all'avvio.
- Sessione in porto con processo vivo, che lo scanner trasforma in «Aperta, pronta per un nuovo compito» (Inattivo → Finito): nessuna notifica e nessun riassunto.
- `claude` assente, scollegato o lento (oltre 60 s): la riga resta sul ripiego, nessun blocco della vedetta, nessun errore ripetuto in coda.
- Testo con password nell'ultima richiesta o nell'ultima risposta: niente invio, riga sul ripiego.

---

### Task 1: Prova delle notifiche sull'app non firmata (prototipo usa e getta)

**Files:**
- Modify: `src-tauri/Cargo.toml`, `src-tauri/src/lib.rs`, `src-tauri/capabilities/default.json`, `package.json`

**Interfaces:**
- Produces: `tauri_plugin_notification` registrato nel builder; permesso `notification:default` nella capability.

- [ ] **Step 1:** `cd src-tauri && cargo add tauri-plugin-notification@2`; registrare `.plugin(tauri_plugin_notification::init())` in `run()`; aggiungere `"notification:default"` ai permessi.
- [ ] **Step 2:** nel `setup`, solo per la prova, mandare una notifica «Crew Deck — prova notifiche» con `app.notification().builder().title(..).body(..).show()`.
- [ ] **Step 3:** `npm run tauri build -- --bundles app`, aprire `src-tauri/target/release/bundle/macos/Crew Deck.app`.
  Expected: macOS chiede il permesso per le notifiche; dopo il sì la notifica compare con il nome «Crew Deck»; il clic porta l'app in primo piano.
- [ ] **Step 4:** se la notifica non compare o risulta di un'altra app: **fermarsi e tornare dall'autore** (vincolo della spec). Se compare: togliere la notifica di prova dal `setup`, tenere plugin e permesso, commit `Notifiche: plugin di Tauri e permesso`.

---

### Task 2: Rilevatore dei passaggi di stato

**Files:**
- Create: `src-tauri/src/segnali.rs`
- Modify: `src-tauri/src/scanner.rs` (campo `inizio_turno` in `Vista`), `src-tauri/src/lib.rs` (`mod segnali;`)
- Test: in `segnali.rs` (`#[cfg(test)]`)

**Interfaces:**
- Consumes: `Vista` (con il nuovo campo `pub inizio_turno: Option<i64>` preso da `Acc::inizio_turno`), `Stato`, `Motivo`.
- Produces:
```rust
pub enum Tipo { Domanda, Permesso, Finito, Errore }
pub struct Evento { pub chiave: String, pub tipo: Tipo, pub progetto: String, pub motivo: String /* = vista.calcolo.azione */ }
#[derive(Default)] pub struct Memoria { /* privato: primo giro fatto, per chiave: stato precedente, turno dell'ultimo permesso notificato */ }
impl Memoria { pub fn passaggi(&mut self, viste: &[Vista], ora: i64) -> Vec<Evento>; }
```

Regole (dalla spec): ingresso in `Aspetta`/`Domanda` → `Domanda` sempre; ingresso in `Aspetta`/`Permesso` → `Permesso` se per quel `inizio_turno` non è già stato notificato un permesso; `Aspetta`/`PermessoStimato` → `Permesso` solo quando `ora - (da_quando - 10_000) >= 30_000` (il `da_quando` stimato è già inizio strumento + 10 s), con lo stesso limite di uno per turno; `Lavora`|`Aspetta` → `Finito`/`FineTurno` con `da_quando - inizio_turno > 120_000` → `Finito`; `Lavora`|`Aspetta` → `Finito`/`ErroreApi` → `Errore`; tutto il resto, incluso `Inattivo` → `Finito` e `Interrotto`, nessun evento. Viste `automatica` o `chiusa` ignorate. Primo giro: memorizza e non emette.

- [ ] **Step 1: test che falliscono** (costruire `Vista` di prova con un helper `vista(chiave, stato, motivo, da_quando, inizio_turno)`):
  - `primo_giro_non_emette`: prima chiamata con una vista in `Aspetta/Domanda` → `vec![]`.
  - `domanda_emette_una_volta`: giro 1 `Lavora`, giro 2 `Aspetta/Domanda` → un evento `Domanda`; giro 3 identico → nessuno.
  - `permessi_a_raffica_uno_per_turno`: alternare `Lavora` e `Aspetta/Permesso` dieci volte con lo stesso `inizio_turno` → un solo evento `Permesso`; con un `inizio_turno` nuovo → un altro.
  - `permesso_stimato_aspetta_30_secondi`: `Aspetta/PermessoStimato` con strumento iniziato 20 s fa → nessuno; stesso stato con 31 s → `Permesso`.
  - `finito_breve_non_emette` (turno di 30 s) e `finito_lungo_emette` (turno di 121 s) → `Finito`.
  - `errore_emette`, `interrotto_non_emette`, `porto_a_finito_non_emette` (`Inattivo` → `Finito`).
  - `automatica_e_chiusa_ignorate`.
- [ ] **Step 2:** `cd src-tauri && cargo test segnali` → FAIL (modulo vuoto).
- [ ] **Step 3:** implementare `Memoria::passaggi` in `segnali.rs` e il campo `inizio_turno` in `Vista`.
- [ ] **Step 4:** `cargo test` → tutti verdi (24 esistenti + nuovi).
- [ ] **Step 5:** commit `Segnali: passaggi di stato per notifiche e riassunti`.

---

### Task 3: Notifiche dal ciclo della vedetta

**Files:**
- Modify: `src-tauri/src/lib.rs`

**Interfaces:**
- Consumes: `segnali::Memoria`, `segnali::Evento`, plugin del Task 1.
- Produces: `fn testo_notifica(e: &Evento, su_cosa: Option<&str>) -> (String, String)` (titolo, corpo) in `lib.rs`.

Testo: titolo «<progetto> ti aspetta» per `Domanda`/`Permesso`, «<progetto> ha finito» per `Finito`, «<progetto> si è fermata per un errore» per `Errore`; corpo = `motivo`, più « · <su_cosa>» se c'è un riassunto salvato.

- [ ] **Step 1: test** `testo_notifica_domanda` (titolo «crew-deck ti aspetta», corpo «Ti fa una domanda · Crew Deck — segnali») e `testo_notifica_senza_riassunto` (corpo solo il motivo).
- [ ] **Step 2:** `cargo test testo_notifica` → FAIL.
- [ ] **Step 3:** implementare `testo_notifica`; nel thread della vedetta tenere una `Memoria`, chiamare `passaggi` dopo `viste(...)` e mostrare una notifica per evento.
- [ ] **Step 4:** `cargo test` → verdi. Prova a mano con `npm run tauri dev`: una sessione Haiku in Ghostty a cui si chiede «chiedimi quale delle due strade preferisco» → notifica entro 3 s; all'avvio dell'app nessuna notifica.
- [ ] **Step 5:** commit `Notifiche quando una sessione aspetta o finisce un lavoro lungo`.

---

### Task 4: Riassunti incrementali con Haiku

**Files:**
- Create: `src-tauri/src/riassunti.rs`
- Modify: `src-tauri/src/lib.rs` (`mod riassunti;`, thread, comando `imposta_riassunti`), `src-tauri/src/scanner.rs` (campi `su_cosa`, `esito` in `Vista`)
- Test: in `riassunti.rs`

**Interfaces:**
- Consumes: `segnali::Evento`, `Scanner::conversazione(agente, id) -> Option<(Option<String>, Option<String>)>` (ultima richiesta, ultima risposta), `carteggio::con_tempo` (rendere `pub(crate)` se non lo è).
- Produces:
```rust
#[derive(Serialize, Deserialize, Clone)] pub struct Riassunto { pub su_cosa: String, pub esito: String, pub fino_a: i64 }
pub fn sensibile(testo: &str) -> bool;                       // porta in Rust la regex SENSIBILE di giudice.py, identica
pub fn richiesta(prec: Option<&Riassunto>, richiesta: Option<&str>, risposta: Option<&str>, attesa: Option<&str>) -> String;
pub fn interpreta(uscita: &str) -> Option<(String, String)>; // JSON {"su_cosa": "...", "esito": "..."}, tagliati a 8 e 12 parole
pub fn leggi(dir: &Path, chiave: &str) -> Option<Riassunto>;
pub fn scrivi(dir: &Path, chiave: &str, r: &Riassunto) -> std::io::Result<()>; // scrittura atomica (temp + rename)
pub struct Coda { /* Sender verso un thread unico */ }
impl Coda { pub fn avvia(dir: PathBuf) -> Coda; pub fn chiedi(&self, lavoro: Lavoro); }
pub struct Lavoro { pub chiave: String, pub richiesta: Option<String>, pub risposta: Option<String>, pub attesa: Option<String>, pub ultima_attivita: i64 }
```
Il prompt per Haiku applica il metodo della spec (formato «argomento — esito», obiettivo attuale con cambi di rotta, «aspetta: …» se c'è un'attesa, priorità obiettivo > decisioni > dettagli, lingua della conversazione) e include il riassunto precedente. Ingresso tagliato: richiesta 2000 caratteri, risposta (coda) 2500. ponytail: si usano solo l'ultima richiesta e l'ultima risposta, la storia viaggia nel riassunto precedente; se serve più contesto si raccolgono tutte le richieste del turno. Il thread salta un lavoro se `ultima_attivita <= fino_a` del riassunto salvato, se `sensibile` scatta su richiesta o risposta, o se l'interruttore è spento; timeout 60 s; un errore non si riprova.

Interruttore: `static RIASSUNTI: AtomicBool` (falso all'avvio) in `riassunti.rs`; comando Tauri `imposta_riassunti(acceso: bool)`.

Accodamento: nel ciclo della vedetta, per ogni evento (`Domanda`, `Permesso`, `Finito`, `Errore`) → `Lavoro` con `attesa = Some(motivo)` per i primi due. All'avvio, dopo il primo giro, un `Lavoro` per ogni vista con `posizione` (viva), non automatica, senza riassunto salvato. Prima di emettere le viste, `su_cosa`/`esito` da `leggi`.

- [ ] **Step 1: test che falliscono**:
  - `sensibile_come_il_giudice`: vero per «non ricordo la password dell'archivio», «ho le pwd del server», «il pin della carta»; falso per «sistema il login».
  - `richiesta_contiene_precedente_e_attesa`: con un precedente `su_cosa = "Crew Deck — notifiche"` e `attesa = Some("Ti fa una domanda")`, il testo contiene entrambi.
  - `richiesta_taglia_ingresso`: risposta di 10 000 caratteri → nel testo al massimo gli ultimi 2500.
  - `interpreta_json_con_recinti`: "```json\n{\"su_cosa\":\"a b\",\"esito\":\"c\"}\n```" → `Some(("a b","c"))`; testo senza JSON → `None`; `su_cosa` di 12 parole → tagliato a 8.
  - `scrivi_e_leggi`: in una cartella temporanea `scrivi` poi `leggi` restituisce lo stesso `Riassunto`; nessun file `.tmp` resta.
- [ ] **Step 2:** `cargo test riassunti` → FAIL.
- [ ] **Step 3:** implementare `riassunti.rs`, i campi in `Vista`, il thread, l'accodamento e `imposta_riassunti`.
- [ ] **Step 4:** `cargo test` → verdi. Prova a mano: con l'interruttore acceso, a fine turno di una sessione Haiku compare `~/.crew-deck/riassunti/claude-<id>.json` entro 20 s; con l'interruttore spento nessun processo `claude -p` parte (`pgrep -fl "claude -p"`).
- [ ] **Step 5:** commit `Riassunti incrementali delle sessioni con Haiku`.

---

### Task 5: Plancia — righe «su cosa» ed «esito», domanda del primo avvio

**Files:**
- Modify: `src/routes/+page.svelte`, `static/anteprima.json` (dati inventati con `su_cosa`/`esito`)

**Interfaces:**
- Consumes: campi `su_cosa: string | null`, `esito: string | null` nella `Vista`; comando `imposta_riassunti`.

- [ ] **Step 1:** tipo `Vista` con `su_cosa`, `esito`. Nella riga: `su_cosa` al posto di `titolo` quando c'è (ripiego: `titolo`, poi `richiesta` tagliata a 60 caratteri); `esito` in una riga sotto, **visibile anche in `li.compatta`** (oggi `.azione` e `.richiesta` sono nascoste lì: aggiungere `.esito` senza nasconderla).
- [ ] **Step 2:** domanda del primo avvio: se `localStorage["crewdeck.riassunti"]` manca, un riquadro come l'invito dell'hook: «Vuoi le righe riassunte? Il testo delle sessioni va a Claude con il tuo abbonamento.» con «Sì» e «No» (predefinito No). La scelta si salva e si manda con `invoke("imposta_riassunti", { acceso })` a ogni avvio. Un interruttore «Riassunti automatici» accanto ai filtri in alto permette di cambiarla.
- [ ] **Step 3:** `npx vite dev` con l'anteprima: riga lunga, riga compatta con esito, riga senza riassunto (ripiego). `npm run check` senza errori.
- [ ] **Step 4:** commit `Plancia: su cosa lavora ed esito su ogni riga, scelta dei riassunti`.

---

### Task 6: Documenti pubblici e rilascio di prova

**Files:**
- Modify: `README.md`, `SPEC.md` (se elenca le funzioni della Plancia), fuori repo `INSTALLA.md` della 0.5.0 (cartella del progetto in superbrain, `output/rilasci/0.5.0/`)

- [ ] **Step 1:** README in inglese: correggere «no network by default»; sezione **Privacy** (cosa esce, quando, come spegnerlo: Jev solo con chiave TypeSafe e testo filtrato; `claude -p` solo su clic per Rifinisci, Approfondisci, Cosa rispondere; riassunti solo se accettati; mai testi con password o credenziali); stato del progetto aggiornato; funzioni notifiche e riga «su cosa»; Credits: billion-context (MIT) per il metodo dei riassunti, con link.
- [ ] **Step 2:** `INSTALLA.md`: un passo sulle notifiche (permesso di macOS al primo avviso) e uno sulla domanda dei riassunti.
- [ ] **Step 3:** `grep -rnE "/Users/|bix12" README.md SPEC.md docs/` → nessun risultato.
- [ ] **Step 4:** `differential-review` sul ramo, poi `codex review`; correggere le segnalazioni confermate (`fp-check` per le importanti).
- [ ] **Step 5:** `npm run tauri build -- --bundles dmg --target universal-apple-darwin` e sostituire il dmg della 0.5.0 (il vecchio nel Cestino). Commit `README: privacy, notifiche e riassunti` e push.

## Fatto quando (dalla spec)
Due sessioni aperte: una finisce un lavoro di oltre 2 minuti, l'altra fa una domanda → due notifiche giuste entro 3 s e, con i riassunti accesi, «su cosa» ed «esito o domanda» corretti su ciascuna riga entro 20 s. Lavoro di 30 s → nessuna notifica. Avvio → nessuna notifica. Interruttore spento → nessuna chiamata a `claude`. Prompt con password → niente invio. Dieci permessi nello stesso turno → una notifica.
