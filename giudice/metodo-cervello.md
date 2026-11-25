<metodo_cervello versione="0.1">
Un cervello è una cartella di file Markdown che gli agenti leggono per conoscere i progetti, ricordare le decisioni e riprendere il lavoro senza perdere il contesto. I file sono la verità: indici e grafi si possono sempre rigenerare.

Struttura da creare nella cartella scelta:

AGENTS.md      regole per gli agenti: cosa possono scrivere, come citano, cosa non toccano
CLAUDE.md      una sola riga che rimanda ad AGENTS.md
INDICE.md      mappa delle pagine, mantenuta dall'agente
REGISTRO.md    diario delle operazioni: cosa è entrato, cosa è stato rivisto, quando
fonti/         testi originali (trascrizioni, estrazioni, appunti grezzi): non si modificano mai dopo l'ingresso
conoscenza/    pagine su progetti, persone, concetti, sintesi; ogni affermazione cita una fonte di fonti/
decisioni/     una decisione per file: data, contesto, decisione, alternative scartate, conseguenze
bozze/         pagine senza fonte o non ancora verificate

Regole da scrivere in AGENTS.md:
1. Ogni frase fattuale in conoscenza/ termina con il riferimento alla fonte, per esempio [F-2026-0412]. Una frase senza fonte va in bozze/ o è segnata come ipotesi.
2. Mai sovrascrivere in silenzio un'affermazione verificata: si aggiunge la nuova versione, si segna la vecchia come superata e si annota il motivo in REGISTRO.md.
3. Lo stato di un lavoro (a che punto è, prossimo passo) vive nel progetto, non nel cervello. Le decisioni durature vanno in decisioni/.
4. Come si crea il cervello locale di un progetto: nella cartella del progetto un AGENTS.md con scopo, stato attuale, prossimo passo e il rimando a questo cervello centrale.
5. A fine sessione: una riga in REGISTRO.md con data, progetto e cosa è stato fatto.
6. Dati personali, credenziali e dati dei clienti non entrano nel cervello senza una ragione chiara.

Come procedere con l'utente:
- Prima spiega in poche frasi cos'è il cervello e cosa stai per creare.
- Chiedi, una domanda alla volta: a cosa servirà (lavoro, progetti personali, clienti), quali progetti vuole collegare per primi, se lavora da solo o con altri.
- Crea la struttura e un AGENTS.md adattato alle risposte. Non inventare contenuti: le pagine di conoscenza nascono vuote o in bozze/.
- Alla fine mostra cosa hai creato e spiega come collegarlo a Crew Deck: Nuova sessione › Cervello centrale › Collega.
</metodo_cervello>
