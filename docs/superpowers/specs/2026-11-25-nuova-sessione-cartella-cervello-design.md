# Nuova sessione: cartella a scelta e cervello centrale facoltativo

Data: 2026-11-25 · Progetto piccolo · Approvato da Fabrizio il 25/11.

## Perché
Chiunque usi Crew Deck deve poter aprire una sessione nuova in qualsiasi cartella, anche nuova. Chi tiene un cervello centrale (una cartella con le regole e il diario del lavoro, per Fabrizio `superbrain/_CERVELLO`) lo collega, così ogni sessione nuova sa dove leggere il contesto, come creare il cervello locale se manca e dove annotare il lavoro. Le regole le decide il cervello centrale, non Crew Deck. Chi non ce l'ha usa Crew Deck come oggi, oppure se ne fa creare uno da un agente con una procedura guidata.

## Cosa si costruisce
1. **Cartella di lavoro**: pulsante «Scegli…» accanto al menu delle cartelle in Nuova sessione. Apre la finestra di macOS (`choose folder`, che ha «Nuova cartella»). La cartella scelta va in testa al menu e resta selezionata.
2. **Cervello centrale** (impostazione unica, localStorage `crewdeck.cervello`), con un «?» che apre una spiegazione breve: cos'è, perché serve agli agenti, cosa succede collegandolo. Si collega con la stessa finestra di macOS; si cambia o si toglie.
3. **Riga in testa al primo messaggio** delle sessioni nuove, visibile sotto le scelte e con una casella per toglierla:
   «Cervello centrale: <percorso>. Prima di cominciare leggi le sue istruzioni. Se questa cartella non ha un cervello locale, chiedi al cervello centrale come si crea e crealo. A fine lavoro annota cosa hai fatto dove il cervello centrale indica.»
   La conferma d'invio dice che la riga c'è. Se la cartella ha un cervello locale lo controlla l'agente.
4. **«Creane uno»** nella spiegazione del «?»: si sceglie una cartella e Crew Deck vi apre una sessione nuova con una richiesta pronta («crea un cervello centrale seguendo questo metodo, spiegami ogni passo e chiedimi ciò che serve») più il metodo incorporato nell'app (`giudice/metodo-cervello.md`), ricavato dalla sezione «Dentro un cervello» dello studio `studio-cervello-ai.md` di Fabrizio. Testo da far rileggere a Fabrizio prima del rilascio; segnato come prima versione.

## Non obiettivi
- Crew Deck non scrive file nel cervello né nei progetti: lo fa l'agente.
- Un solo cervello centrale; più cervelli (per riservatezza) dopo, quando lo studio sarà definitivo.
- Niente riga per le sessioni già aperte.

## Approcci considerati
1. Riga in testa al primo messaggio — scelto: visibile, uguale per Claude, Codex e pi.
2. Istruzione nascosta all'avvio (`--append-system-prompt`, `-c` di Codex; per pi non c'è): invisibile e diversa per agente. Di riserva.
3. File scritto da Crew Deck nella cartella del progetto: Crew Deck scriverebbe nei progetti altrui. Scartato.
Per la creazione: procedura fatta dall'agente (scelta) invece di Crew Deck che crea le cartelle da sé, che fisserebbe uno schema senza spiegare.

## Scelte mie (correggibili)
- Comando Tauri `scegli_cartella(titolo)` con `osascript choose folder`; Annulla → nessun cambio. Nessuna dipendenza nuova.
- Il metodo per «Creane uno» viaggia come il prompt: file temporaneo letto dal comando di avvio, come oggi in `nuova_sessione`.
