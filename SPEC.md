# Crew Deck — specifica v0.1

> La plancia dell'equipaggio: vedi ogni agente AI al lavoro, sai chi ha finito e chi ti aspetta, e impari a chiedere meglio.
> App desktop open source (Tauri), macOS · Windows · Linux, interfaccia IT/EN. Brainstorming completo in `BRAINSTORM.md`.

## 1. Problema
Chi lavora con più agenti AI insieme (Claude Code, Codex, pi…) perde il filo: quale sessione sta lavorando e su quale cartella, chi ha finito, chi è fermo in attesa di una risposta, quanto si sta spendendo. E non ha un modo per capire se il suo modo di chiedere funziona.

## 2. Principi
1. **Locale e in sola lettura.** Legge i registri che gli agenti già scrivono su disco. Non tocca le sessioni. Nessun account, nessun server.
2. **Niente rete per default.** Le uniche uscite sono quelle che l'utente attiva: l'analisi AI delle richieste e il Porto servizi.
3. **Privacy prima.** Prima di mandare testo a un modello si oscurano percorsi, nomi utente e chiavi. Le chiavi stanno solo nel portachiavi di sistema.
4. **Stato spiegato.** Ogni stato ha un motivo leggibile ("chiede il permesso per: avviare i test"), e si dice quando è una stima.
5. **Leggero.** Sempre acceso senza pesare: CPU vicina a zero a riposo, niente polling aggressivo.

## 3. Fonti (adattatori)
Un adattatore per agente trasforma il suo registro nel modello comune (§4). Si aggiungono agenti senza toccare il resto.

| Agente | File | Cartella di lavoro | Richiesta | Fine turno | Token/costo |
|---|---|---|---|---|---|
| Claude Code (CLI e app, modalità Code) | `~/.claude/projects/*/*.jsonl` (+ `subagents/`) | `cwd` | `type:user`, `promptSource:typed` | ultimo `assistant` senza tool aperti, `stop_reason` | `message.usage` |
| Codex (CLI e app) | `~/.codex/sessions/AAAA/MM/GG/rollout-*.jsonl` | `turn_context.cwd` | `item_completed` → `UserMessage` | `event_msg.task_complete` | `event_msg.token_count` |
| pi | `~/.pi/agent/sessions/<cwd>/*.jsonl` | `session.cwd` | `message` ruolo `user` | `stopReason` dell'`assistant` | `usage` con `cost.total` |

Note:
- I formati non sono documentati come API stabili: ogni adattatore ha test con registri reali anonimizzati (`fixtures/`) e tollera campi sconosciuti.
- Chat normali di Claude.ai e ChatGPT: stanno sui server, **non sono leggibili**. Si dichiara nell'interfaccia.
- Lettura incrementale: si legge solo la coda nuova di ogni file (offset salvato), osservando le cartelle con il watcher del sistema operativo.

## 4. Modello comune
```
Sessione   id, agente, cartella (cwd), progetto (nome cartella / repo git), ramo git,
           titolo, modello, inizio, ultima_attività, stato, motivo, stimato (bool),
           richiesta_corrente, ultima_azione, strumento_attivo, sotto_agenti[],
           token {input, output, cache_lettura, cache_scrittura}, costo_usd (se noto)
Richiesta  id, sessione, testo, ora, durata_turno, n_strumenti, n_errori, token, costo,
           esito {completata, interrotta, errore, correzione_successiva}
Evento     sessione, ora, tipo, dettaglio      (solo in memoria + ultimi N su disco)
```

## 5. Stati
| Stato | Icona | Regola (prima che vale, in ordine) |
|---|---|---|
| **Chiama lo skipper** (aspetta te) | 🚩 | domanda aperta all'utente · richiesta di permesso (certa con hook, stimata se uno strumento veloce è fermo da >10 s) |
| **In navigazione** (lavora) | ⛵ | strumento in corso · sotto-agente al lavoro · modello che pensa da meno di 10 min · comando in background |
| **Al traguardo** (finito) | 🏁 | turno chiuso · interrotto da te · errore API (con motivo) |
| **In porto** (inattivo) | ⚓ | nessun evento da 30 min · sessione chiusa |

Il motore è una funzione pura (registro → stato + motivo), testata caso per caso. Riferimento di partenza: la logica di agent-tower (`_ref/`, MIT), rifatta in Rust e generalizzata ai tre agenti.

## 6. Funzioni

### 6.1 Torretta (sempre accesa)
Finestra compatta sempre in primo piano: una riga per sessione attiva (icona del ruolo, progetto, stato, da quanto). Una **barca animata** veleggia finché qualcuno lavora e si ferma quando tutti hanno finito. Icona nella barra dei menu colorata come lo stato più urgente. Modalità fantasma (trasparente, il mouse ci passa attraverso). Clic → si espande in dashboard.

### 6.2 Dashboard
- **Equipaggio**: schede per sessione, raggruppate per progetto; dettaglio con richiesta corrente, ultime azioni, file toccati, token e costo.
- **Rotta**: cronologia della giornata per progetto.
- **Conti**: token e costi per agente, progetto, giorno.

### 6.3 Ruoli d'equipaggio (dinamici)
Il ruolo viene da cosa l'agente sta facendo, dagli strumenti usati nell'ultimo turno:
| Ruolo | Quando |
|---|---|
| Tattico | pianifica, legge molto, scrive poco |
| Navigatore | cerca sul web, legge documentazione |
| Prodiere | esplora il codice (ricerche, letture in serie) |
| Grinder | scrive e modifica file |
| Randista | esegue comandi, test, build |
| Timoniere | coordina sotto-agenti |
Lo skipper sei tu. Regole in una tabella modificabile, non nel codice.

### 6.4 Notifiche
Notifica di sistema quando una sessione passa a 🚩 o a 🏁 dopo più di N minuti di lavoro. Silenziabili per progetto. Suono opzionale (fischio da regata).

### 6.5 Azioni (fatto il 23/11 per macOS)
- **Apri in <app>**: se la sessione è viva, Crew Deck sa dove gira (processo → app ospite → tty) e porta in primo piano **la scheda esatta**: Ghostty, Terminale e iTerm via AppleScript sul tty; cmux via `select-workspace` + `focus-panel` usando il registro `cmux sessions list` (collegamento esatto per id di sessione). cmux accetta comandi da fuori solo in modalità password (Impostazioni › Automazione; password nel portachiavi, servizio `crew-deck`, account `cmux`); altrimenti porta cmux in primo piano. Altre app (VS Code, app Claude/Codex): attivate.
- **Riprendi**: se è chiusa, apre una scheda nuova nella cartella della sessione, nell'ultima app dove girava (Ghostty di default), con `claude --resume <id>`, `codex resume <id>`, `pi --session <file>`; lanciato solo con un clic esplicito.
- **Apri la cartella** nel file manager o nell'editor.
- Fermare un agente: fuori dalla v1 (rischioso, nessun modo pulito comune ai tre).

### 6.6 Debriefing — analisi delle richieste
- **Statistiche** (locali, sempre): lunghezza delle richieste; turni per arrivare al risultato; correzioni subito dopo («no», «non così», nuova richiesta sullo stesso tema); richieste interrotte; strumenti ed errori per richiesta; costo per richiesta; differenze fra agenti e progetti.
- **Analisi AI** (opt-in): un campione di richieste, oscurate, va a un modello con l'abbonamento dell'utente (`claude -p` o `codex exec`, nessuna chiave da gestire). Torna con schemi ricorrenti e riscritture suggerite. Opzione **Jev (TypeSafe)** per giudizi tipizzati a basso costo, una domanda per criterio: la richiesta dice il criterio di fine? nomina i file? contiene due richieste in una?
- **Giornale di bordo**: report settimanale (cosa hai chiesto, cosa ha funzionato, tre consigli).

### 6.7 Carta nautica — salute del cervello dei progetti
Per ogni cartella in cui lavorano gli agenti: esistono `AGENTS.md`/`CLAUDE.md`/`HANDOFF.md`? quando sono stati aggiornati rispetto all'ultimo lavoro? sono troppo lunghi? `graphify-out` è più vecchio dell'ultimo commit? Contraddizioni fra documenti: opt-in, con modello o Jev. Ogni avviso ha un'azione ("copia il prompt per sistemarlo").

### 6.8 Porto servizi e Cassaforte (fase 5)
- **Cassaforte**: chiavi API nel portachiavi di sistema, mai in chiaro su disco. Quando un agente si blocca per una chiave mancante, Crew Deck avvisa, l'utente la inserisce nell'app, e l'app la scrive dove serve (`.env` del progetto o variabile) solo dopo conferma. L'agente non vede mai il valore in chat.
- **Porto servizi**: catalogo di server MCP e CLI (prima Perplexity e Firecrawl). Un clic configura il servizio per Claude Code, Codex e pi, prendendo la chiave dalla Cassaforte. Stato per collegamento: attivo, errore, chiave scaduta.

## 7. Architettura
```
crew-deck/
  src-tauri/            Rust
    adapters/           claude.rs · codex.rs · pi.rs   (registro → modello comune)
    engine/             stato.rs (funzione pura) · ruoli.rs · costi.rs
    store/              SQLite (rusqlite): sessioni, richieste, offset dei file
    watch/              notify: osserva le cartelle dei registri
    actions/            terminale, riprendi, apri cartella
    vault/              keyring (fase 5)
  src/                  interfaccia: Svelte 5 + TypeScript + Vite
    tower/ · deck/ · debrief/ · chart/ · harbor/
    i18n/ it.json · en.json
  fixtures/             registri reali anonimizzati, uno per caso di stato
```
- Il backend fa tutto il lavoro pesante e manda all'interfaccia solo dati pronti (eventi Tauri).
- Permessi Tauri minimi: lettura delle sole cartelle dei registri, nessuna rete salvo le funzioni opt-in.

## 8. Fasi e criteri di fine
| Fase | Contenuto | Fatto quando |
|---|---|---|
| **1. Vedetta** | adattatori Claude/Codex/pi, motore di stato, elenco sessioni con stato, cartella, richiesta, token | le sessioni reali di questo Mac compaiono con lo stato giusto in tutti i casi di `fixtures/`; CPU < 1% a riposo |
| **2. Plancia** | torretta + dashboard, tema regata, ruoli, barca animata, notifiche, azioni | una sessione che passa a 🚩 genera la notifica; «apri terminale» funziona con Ghostty |
| **3. Debriefing** | statistiche, analisi AI opt-in, giornale di bordo | report settimanale generato sulle sessioni reali, con testi oscurati |
| **4. Carta nautica** | salute di cervello e grafo | segnala correttamente un `HANDOFF` vecchio e un grafo scaduto su un progetto di prova |
| **5. Porto e Cassaforte** | MCP/CLI, chiavi | Perplexity configurato per i tre agenti con la chiave dal portachiavi |
| **Rilascio** | build firmate per mac/win/linux via GitHub Actions, README, licenza MIT | release v1.0 scaricabile |

## 9. Decisioni prese (22/11/2026)
1. Interfaccia: **Svelte 5** + TypeScript.
2. Repo pubblico `crew-deck`, licenza **MIT**.
3. Hook opzionali di Claude Code per gli stati certi: **sì, in fase 2**, solo dopo conferma dell'utente e con backup delle impostazioni.
4. Distribuzione come pacchetto nativo per sistema (niente container: è un'app desktop).
