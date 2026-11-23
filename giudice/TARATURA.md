# Taratura del giudice dei prompt — 23/11/2026

Modello `jev-1.14.0` (TypeSafe). 40 prompt veri estratti a caso da Claude Code, Codex e pi, più 10 casi di controllo scritti a mano con risposta evidente. Etichette scritte prima di vedere le risposte di Jev. I testi restano fuori da git (`taratura/`); qui solo i numeri.

## Riservatezza
- 10 prompt su 40 **non sono partiti**: parlavano di password o credenziali, nel prompt o nell'ultimo messaggio dell'agente. Un prompt conteneva indizi di password senza nessuna parola rivelatrice, riconoscibile solo dal messaggio dell'agente che li chiedeva: per questo la regola guarda entrambi.
- Ai restanti si tolgono percorsi (anche con spazi nei nomi), email, telefoni, IBAN, codici fiscali, chiavi, link.

## Prestazioni
Latenza mediana 0,24 s (massima 0,66 s) per 7 domande in una richiesta. Circa 1.200 token in ingresso e 130 in uscita per prompt.

## Accordo con le etichette (secondo giro, definizioni corrette)
| Punto | Prompt veri | Controllo | Uso consigliato |
|---|---|---|---|
| Tipo (compito nuovo / domanda / conferma o scelta / risposta / commento) | 21/25 | 10/10 | affidabile; errori solo su casi di confine |
| Come si verifica | 10/10 | 5/5 (con positivi) | **affidabile**: è il punto più importante per Anthropic |
| Una richiesta alla volta | 8/8 | 5/5 | affidabile |
| Vincoli | 7 ok · 1 no · 2 incerti | 5/5 | affidabile |
| Contesto che l'agente non vede | 9 ok · 1 no | 3 ok · 1 no · 1 incerto | discreto |
| Perché serve | 6 ok · 1 no · 3 incerti | 2 ok · 3 incerti | solo come suggerimento |
| Dove intervenire | 3 ok · 2 no · 5 incerti | 4 ok · 1 incerto | debole sui prompt conversazionali: il «dove» spesso sta nella conversazione, non nel prompt |

## Cosa è cambiato fra il primo e il secondo giro
- «È un compito?» (sì/no) → scelta fra cinque tipi: le scelte fra opzioni proposte dall'agente non sono compiti nuovi.
- «Dove» comprende progetto, documento e schermata, non solo file e cartelle.
Cambiate le definizioni, non le parole per ottenere un numero. Il secondo giro è sullo stesso campione: i casi di controllo sono l'unica verifica fuori campione, e sono pochi.


## Il dato che conta
Fra i compiti veri del campione, **nessuno** diceva come verificare il risultato. È il primo consiglio da dare.

## Verifica fuori campione (terzo giro, 30 prompt mai visti)
28 valutabili: 2 erano messaggi di sistema (notifica di fine attività, comando `/mcp`) e vanno saltati prima di Jev.
| Punto | Esito | Nota |
|---|---|---|
| Tipo | **21/23 (91%)** | confermato |
| Come si verifica | 7 ok · 1 incerto · 0 errori | confermato |
| Contesto | 5 ok · 1 «errore» · 1 incerto | sul caso in disaccordo aveva ragione Jev (una preferenza dell'utente è contesto) |
| Vincoli | 6 ok · 1 «errore» · 1 incerto | il disaccordo è difendibile |
| Perché serve | 6 ok · 3 incerti | solo informazione |
| Una richiesta alla volta | 2 ok · 1 no · 6 incerti | **non regge fuori campione**: solo informazione |
| Dove intervenire | 4 ok · 3 no · 2 incerti | debole: solo informazione |

## Regola d'uso definitiva (v1)
- Prima il tipo: i punti valgono solo per `new_task`.
- **Avvisi** solo per «come si verifica» e «vincoli», quando Jev dice chiaramente no (p ≤ 0,3).
- «Contesto» e «perché» si mostrano come informazione; «dove» e «una richiesta alla volta» si registrano ma non si mostrano finché una nuova taratura non li rende affidabili.
- Saltati prima di Jev: messaggi che iniziano con `<` (sistema), prompt di 3 parole o meno, argomenti sensibili.
