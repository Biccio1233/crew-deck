# Nuova sessione: cartella a scelta e cervello facoltativo

Data: 2026-11-25 · Piccola modifica · Approccio scelto da Fabrizio: 1 (riga in testa al primo messaggio).

## Perché
Chiunque usi Crew Deck deve poter aprire una sessione nuova in qualsiasi cartella, anche in una che non esiste ancora. Chi tiene un «cervello» (una cartella con istruzioni e diario del lavoro, come `superbrain/_CERVELLO`) deve poterlo collegare, così la sessione nuova sa dove leggere il contesto e dove annotare cosa ha fatto. Chi non ce l'ha usa Crew Deck come oggi.

## Obiettivi
- Pulsante «Scegli…» accanto al menu delle cartelle: apre la finestra standard di macOS (`choose folder`, che ha già «Nuova cartella»). La cartella scelta entra in testa al menu e resta selezionata.
- Cervello facoltativo: «Collega un cervello…» sceglie la cartella con la stessa finestra; Crew Deck la ricorda (localStorage `crewdeck.cervello`). Si può cambiare o togliere.
- Con un cervello collegato, sotto le scelte compare la riga che verrà messa in testa al messaggio, con una casella per non aggiungerla a quella sessione. La conferma d'invio mostra che la riga c'è.
- Riga: «Cervello di riferimento: <percorso>. Prima di cominciare leggi lì le istruzioni; a fine lavoro annota lì cosa hai fatto.»

## Non obiettivi
- Crew Deck non scrive nel cervello né nei progetti e non tiene un diario suo.
- Niente riga per le sessioni già aperte: il cervello vale solo per «Nuova sessione…».
- Niente istruzione nascosta all'avvio dell'agente (approccio 2), niente file nel progetto (approccio 3).

## Approcci considerati
1. Riga in testa al primo messaggio — scelto: visibile, uguale per Claude, Codex e pi, minimo codice. Limite: in sessioni lunghe l'agente può perderla di vista.
2. Istruzione nascosta all'avvio (`--append-system-prompt` per Claude, `-c` per Codex; per pi non c'è): valida per tutta la sessione, ma invisibile e diversa per agente. Si passa a questa se la 1 non basta.
3. File nella cartella del progetto che rimanda al cervello: persistente, ma Crew Deck scriverebbe nei progetti altrui.

## Scelte mie (correggibili)
- Comando Tauri `scegli_cartella(titolo)` con `osascript choose folder`; Annulla → nessun cambio. Nessuna dipendenza nuova.
- Un'unica casella «Aggiungi il cervello» per invio, accesa di default.
