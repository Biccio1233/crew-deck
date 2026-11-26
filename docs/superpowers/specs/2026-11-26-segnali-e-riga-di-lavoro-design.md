# Segnali di fine lavoro e attesa, riga «su cosa lavora»

Data: 2026-11-26 · Progetto · Scelte dell'autore del 26/11 incluse; in attesa di approvazione del documento.

## Perché
L'autore tiene aperte molte sessioni (Claude Code, Codex, pi) e lavora in altre finestre. Oggi la Plancia sa già quando una sessione ha finito o aspetta una risposta, ma il segnale resta dentro la finestra di Crew Deck: se la finestra è coperta, l'autore se ne accorge solo andando a controllare. Inoltre la riga di una sessione non dice su cosa sta lavorando: il titolo generato da Claude è spesso generico («Riprendiamo») e resta quello della prima frase, Codex di solito non ha titolo, e nelle righe compatte (finite o ferme) l'ultima richiesta è nascosta. Crew Deck è un cruscotto di gestione: deve dire da sé chi ha bisogno di lui e far capire a colpo d'occhio cosa fa ciascuna sessione.

## Obiettivi
1. **Notifica di macOS** quando una sessione passa a «ti aspetta» (domanda o permesso) e quando finisce un lavoro durato più di 2 minuti.
2. **Due righe per sessione**, sempre visibili anche nelle righe compatte:
   - **su cosa** — l'obiettivo della sessione com'è adesso, al massimo 8 parole («Crew Deck — segnali e riga di lavoro»);
   - **esito o domanda** — al traguardo l'esito del turno, in attesa «aspetta: <cosa>» (al massimo 12 parole).
3. Le due righe le scrive Haiku (`claude -p`) con un riassunto **incrementale**, secondo il metodo ricavato da billion-context (sotto). Interruttore per spegnere i riassunti automatici.
4. Prima di accendere le notifiche, eliminare i **falsi «ti aspetta»**: oggi sessioni di collaudo chiuse da un giorno risultano ancora «Ti fa una domanda».

## Non obiettivi
- Nessun suono, nessun numero sull'icona del Dock o della barra dei menu (resta alla 0.6).
- Nessuna notifica per le sessioni automatiche (lanciate da altri programmi) né per le sessioni chiuse.
- Nessuna notifica all'avvio dell'app per stati già presenti: solo per i passaggi avvenuti mentre l'app è aperta.
- Nessuna pagina di impostazioni completa (resta alla 0.6): solo l'interruttore dei riassunti.
- Il riassunto non torna mai all'agente: lo legge solo l'autore.
- Nessun cambio alle altre parti della Plancia (sezioni, filtri, azioni), al Carteggio e al Debriefing.

## Anti-obiettivi (fallimento anche se «funziona»)
- Notifiche che sommergono: una per ogni risposta breve, o ripetute per lo stesso stato.
- Notifiche false («ti aspetta» per una sessione che non aspetta).
- Testo con password o credenziali inviato a Claude.
- Riassunti che costano in modo visibile: una chiamata per ogni rilettura invece che per ogni passaggio di stato.

## Approcci considerati
**Riassunto della riga**
1. **Riassunto automatico con Haiku, incrementale** — scelto dall'autore. Righe chiare e aggiornate; costo contenuto perché si manda solo la parte nuova. Cambia la regola di privacy: il testo esce dal Mac senza un clic, quindi c'è l'interruttore.
2. Nessun modello: titolo, ultima richiesta e ultima frase dell'agente tagliati corti. Gratis e senza invii, ma poco chiaro e fermo alla prima frase. Resta come ripiego quando i riassunti sono spenti o non ancora pronti.
3. Usare billion-context stesso: richiede di far passare il traffico degli agenti da un suo intermediario. Scartato: Crew Deck legge solo i registri.

**Segnale fuori dalla finestra**
1. **Notifica di macOS** — scelta dall'autore.
2. Solo numero sull'icona: discreto ma va guardato. Rinviato alla 0.6.
3. Tutte e due: rinviato con il numero.

## Metodo dei riassunti (da billion-context, `kernel/src/compression-rules.ts`, MIT)
Si prende il metodo, non il codice:
- **Formato del terzo livello**: «argomento — esito in ≤8 parole» → la riga «su cosa».
- **Obiettivo com'è alla fine, con i cambi di rotta**: a ogni giro «su cosa» si riscrive secondo l'obiettivo attuale, non la prima frase.
- **«Obiettivi aperti»**: le richieste non chiuse → «aspetta: …» quando la sessione aspetta; sparisce quando la richiesta è chiusa.
- **Incrementale**: in ingresso solo il riassunto precedente + le richieste nuove del turno + l'ultima frase dell'agente, mai l'intera sessione.
- **Priorità quando manca spazio**: obiettivo e vincoli, poi decisioni, poi dettagli.
- **Lingua**: la lingua della conversazione (regola «language preservation»).
Restano fuori: l'intermediario, gli strumenti compress/decompress, i livelli 1 e 2, la cache.

## Cosa si costruisce
1. **Falsi «ti aspetta»**: trovare perché i segnali di sessioni chiuse restano validi oltre le 2 ore previste (`scanner.rs`, `leggi_attese`; `engine.rs`) e correggere. Prova: le sessioni `collaudo-*` di ieri non risultano più «Ti fa una domanda».
2. **Passaggi di stato**: il motore confronta lo stato di ogni sessione con quello del giro precedente e produce gli eventi «ora aspetta» e «ora ha finito» (con la durata del turno). Il primo giro dopo l'avvio non produce eventi.
3. **Notifiche** (il clic porta Crew Deck in primo piano):
   - **Domanda** (certa, dall'hook): sempre.
   - **Permesso certo** (hook di Claude): al massimo una notifica per sessione per turno.
   - **Permesso stimato** (Codex e pi, strumento fermo): solo dopo 30 secondi di fermo, e anche qui al massimo una per turno. Scelta dell'autore.
   - **Ha finito**: solo il lavoro concluso davvero (`FineTurno`) durato più di 2 minuti, e l'errore del servizio (`ErroreApi`, «si è fermata per un errore»). Niente notifica per `Interrotto` (l'ha fermata l'autore) né per il «finito» artificiale di `scanner.rs` (sessione in porto con processo vivo).
   - **Testo**: «<progetto> ti aspetta» / «<progetto> ha finito», con sotto il motivo dello stato (disponibile subito) e la riga «su cosa» dell'ultimo riassunto salvato. Non si aspetta il riassunto nuovo, così la notifica resta entro 3 secondi.
4. **Riassunti**: a ogni passaggio verso «ha finito» (`FineTurno`, `ErroreApi`) o «aspetta», per le sessioni umane, una richiesta in coda (una alla volta) a `claude -p --model haiku` con gli stessi parametri chiusi di `carteggio.rs` (niente strumenti, niente sessione salvata, niente impostazioni). Ingresso: riassunto precedente + richieste nuove + ultima frase dell'agente, tagliato a una lunghezza fissa. Uscita: due righe in formato fisso. Il filtro delle password già usato per Jev si applica all'ingresso: se scatta, niente invio e si usa il ripiego. Salvataggio in `~/.crew-deck/riassunti/<agente>-<id>.json` con il punto del registro già riassunto. All'avvio dell'app si riassumono una volta sola le sessioni vive senza riassunto salvato (il primo senza riassunto precedente); le sessioni chiuse mai riassunte restano sul ripiego.
5. **Riga in Plancia**: «su cosa» al posto del titolo quando c'è; «esito o domanda» visibile anche nelle righe compatte. Ripiego: titolo o ultima richiesta tagliati.
6. **Interruttore** «Riassunti automatici», salvato in localStorage come le altre preferenze. Al primo avvio l'app chiede: «Vuoi le righe riassunte? Il testo delle sessioni va a Claude con il tuo abbonamento», con risposta predefinita **no**. Nessuna chiamata parte prima della risposta. Vale per tutti, autore compreso (che risponderà sì): una sola build, nessuna costante di compilazione. Scelta dell'autore.
7. **README pubblico** (il progetto è open source, il README va scritto bene): oggi dice «no network by default» e lo stato «phase 1», entrambi superati. Si aggiunge una sezione **Privacy** precisa su cosa esce dal Mac e quando: testo filtrato dei prompt verso Jev (solo con chiave TypeSafe), richieste a `claude -p` per Rifinisci, Approfondisci e Cosa rispondere (solo su clic), riassunti automatici (solo se accettati), e cosa non esce mai (prompt con password o credenziali). Si aggiornano lo stato del progetto e le funzioni (notifiche, riga «su cosa»), e nei Credits si cita billion-context (MIT) per il metodo dei riassunti. In inglese, come il resto del README.

## Fatto quando
- Due sessioni aperte: una finisce un lavoro di oltre 2 minuti, l'altra fa una domanda. Arrivano due notifiche giuste entro 3 secondi dal passaggio di stato, e su ciascuna riga compaiono «su cosa» ed «esito o domanda» corretti entro 20 secondi.
- Un lavoro di 30 secondi che finisce non produce notifiche. All'avvio dell'app non arriva nessuna notifica.
- Con l'interruttore spento non parte nessuna chiamata a `claude` e la riga mostra il ripiego.
- Un prompt che parla di password non viene inviato (la riga usa il ripiego).
- Le sessioni chiuse di ieri non risultano «ti aspetta».
- Al primo avvio compare la domanda sui riassunti; finché non si risponde, nessuna chiamata a `claude`.
- Dieci permessi nello stesso turno producono una sola notifica; un permesso stimato notifica solo dopo 30 secondi di fermo.
- Test automatici per: eventi di passaggio (incluso il primo giro), soglia dei 2 minuti, limite di una notifica di permesso per turno, ritardo dei permessi stimati, filtro password sull'ingresso del riassunto, lettura e scrittura del file dei riassunti.

## Vincoli
- Una sola dipendenza nuova ammessa: `tauri-plugin-notification`. Motivo: con `osascript display notification` la notifica risulta di «Script Editor» e il clic apre Script Editor invece di Crew Deck.
- Pacchetto non firmato: verificare per primo, con un prototipo, che macOS mostri le notifiche e chieda il permesso. Se non funziona si torna dall'autore prima di andare avanti.
- Privacy: README (punto 7) e `INSTALLA.md` dicono cosa esce, quando e come spegnerlo. Le regole del progetto («Privacy» in AGENTS.md) si aggiornano con i riassunti automatici.
- Sicurezza: `differential-review` prima del merge e secondo parere di Codex, come ogni modifica.

## Altri agenti ed editor (richiesta dell'autore del 26/11)
Crew Deck deve poter accogliere altri agenti (Gemini CLI, OpenCode, Copilot CLI, Cursor…) e sessioni che girano negli editor (VS Code, Cursor, Zed). In questo progetto se ne tiene conto così:
- Notifiche e riassunti lavorano solo sugli stati comuni (`Stato`, motivo, turno, ultime richieste e ultima frase dell'agente) prodotti dai lettori, mai su campi di un agente preciso. Un agente nuovo li ottiene appena il suo lettore produce gli stati, senza toccare questa parte.
- La regola dei permessi segue la fonte, non il nome dell'agente: segnale certo (un hook, oggi solo Claude) → regola dei certi; deduzione dal registro → regola degli stimati (30 secondi).
- Il clic sulla notifica porta in primo piano Crew Deck, non un terminale: vale qualunque sia l'editor o il terminale della sessione.
- Le sessioni di Claude Code dentro un editor (estensione per VS Code, `entrypoint: claude-vscode`) finiscono già negli stessi registri: ricevono notifiche e riassunti come le altre.
Fuori da questo progetto, in uno successivo: i lettori dei nuovi agenti e l'«Apri» negli editor (riconoscere che una sessione gira in VS Code, Cursor o Zed e aprirla lì).

## Lasciato a chi costruisce
- Lunghezza massima dell'ingresso del riassunto e testo esatto del prompt per Haiku (in linea con il metodo sopra).
- Come si rappresentano i passaggi di stato nel motore e come arrivano all'interfaccia.
- Posizione esatta dell'interruttore nella Plancia.
