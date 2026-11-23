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

## Regola d'uso in Crew Deck
Segnalare un punto mancante solo se Jev dice chiaramente no (p ≤ 0,3), e solo per: come si verifica, una richiesta alla volta, vincoli, perché. «Dove» e «contesto» si mostrano, ma non generano avvisi.

## Il dato che conta
Fra i compiti veri del campione, **nessuno** diceva come verificare il risultato. È il primo consiglio da dare.

## Da fare
Rifare la taratura con 30 casi nuovi mai visti, prima di attivare l'hook su tutte le sessioni.
