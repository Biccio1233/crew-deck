#!/usr/bin/env python3
"""Giudice dei prompt: Jev (TypeSafe) giudica una richiesta fatta a un agente AI, punto per punto.

Jev dà i giudizi (una domanda sì/no per punto, con probabilità tarata); cosa farne lo decide
il codice di Crew Deck o l'utente. Solo libreria standard.

    python3 giudice.py "testo del prompt" [--prima "ultimo messaggio dell'agente"]
    python3 giudice.py --payload "testo"     # mostra cosa partirebbe, senza rete
    python3 giudice.py --prova               # filtro e soglie, senza rete
    python3 giudice.py --filtra < testo      # stampa il testo con i FILTRI applicati, senza rete

Chiave: variabile TYPESAFE_API_KEY, altrimenti Portachiavi macOS (servizio TYPESAFE_API_KEY).
"""
from __future__ import annotations

import json
import os
import re
import subprocess
import sys
import time
import urllib.error
import urllib.request

URL = "https://api.typesafe.ai/v1/systemone"
MODELLO = os.environ.get("CREWDECK_JEV_MODEL", "jev-latest")

# Soglie di partenza consigliate da TypeSafe per un Noul: sopra 0.7 sì, sotto 0.3 no, in mezzo incerto.
SI, NO = 0.7, 0.3

# ---------------------------------------------------------------------------
# Le domande, tutte qui. Ogni punto viene dalle guide di Anthropic (Claude Code best practices)
# e OpenAI (prompting per Codex e GPT-5). Una domanda = un giudizio che una persona darebbe a colpo d'occhio.
# ---------------------------------------------------------------------------
DOMANDE = {
    "tipo": {
        "type": "choice",
        "instructions": "What kind of message is `request`, given what the agent said in `agent_last_message`?",
        "criteria": {
            "new_task": "Asks the agent to do, build, change, fix, analyse, write or research something the agent had not already proposed.",
            "question": "Asks the agent for information or an opinion, without asking it to produce or change anything.",
            "approval_or_choice": "Approves what the agent proposed or picks one of the options it offered, for example 'ok go ahead', 'let's try it locally', 'keep databot', 'go with B'.",
            "answer_to_agent": "Answers a question the agent asked, giving facts or preferences.",
            "comment": "A greeting, thanks, praise, or a remark that asks for nothing.",
        },
    },
    "criterio_fine": {
        "type": "noul",
        "instructions": "Does `request` say how to tell that the work is done or correct?",
        "criteria": {
            "true": "It names a check the result must pass: a test or command to run, an expected output, an example of the result, or a measurable condition.",
            "false": "It says what to do but gives no way to check the result.",
        },
    },
    "perche": {
        "type": "noul",
        "instructions": "Does `request` explain why the result is needed or what it will be used for?",
        "criteria": {
            "true": "It states a goal, a purpose, or who will use the result and for what.",
            "false": "It only states the action, with no purpose.",
        },
    },
    "contesto": {
        "type": "noul",
        "instructions": "Does `request` give facts the agent could not find by reading the project files?",
        "criteria": {
            "true": "It shares a decision already taken, a preference, an observed symptom or error, an external constraint, or information only the user has.",
            "false": "Everything it mentions could be discovered by the agent on its own.",
        },
    },
    "dove": {
        "type": "noul",
        "instructions": "Does `request` name the specific thing to work on?",
        "criteria": {
            "true": "It names the project, repository, document, file, folder, screen, function or exact element the work is about.",
            "false": "It describes the work in general terms and the agent has to guess what it applies to.",
        },
    },
    "vincoli": {
        "type": "noul",
        "instructions": "Does `request` set limits or say what to avoid?",
        "criteria": {
            "true": "It says what must not change, what to avoid, a size, time or budget limit, or what is out of scope.",
            "false": "It sets no limits.",
        },
    },
    "piu_compiti": {
        "type": "noul",
        "instructions": "Does `request` ask for two or more separate tasks that could each be done on their own?",
        "criteria": {
            "true": "It bundles independent tasks, for example 'fix the login and also redesign the homepage'.",
            "false": "It asks for one task, possibly with several steps that serve that same task.",
        },
    },
}

NOMI = {
    "criterio_fine": "Come si verifica",
    "perche": "Perché serve",
    "contesto": "Contesto che l'agente non vede",
    "dove": "Dove intervenire",
    "vincoli": "Vincoli",
    "piu_compiti": "Una richiesta alla volta",
}


# ---------------------------------------------------------------------------
# Riservatezza: cosa non parte mai
# ---------------------------------------------------------------------------
FILTRI = [
    (re.compile(r"[\w.+-]+@[\w-]+\.[\w.-]+"), "<email>"),
    (re.compile(r"\b(sk|pk|ghp|gho|xox[bp]|AKIA|AIza)[-_A-Za-z0-9]{12,}"), "<chiave>"),
    (re.compile(r"\b[0-9]{8,10}:AA[A-Za-z0-9_-]{30,}"), "<chiave>"),
    (re.compile(r"\bIT\d{2}[A-Z]\d{10}[0-9A-Z]{12}\b", re.I), "<iban>"),
    (re.compile(r"\b[A-Z]{6}\d{2}[A-Z]\d{2}[A-Z]\d{3}[A-Z]\b", re.I), "<codice fiscale>"),
    (re.compile(r"(?<!\w)(\+39\s?)?3\d{2}[\s.-]?\d{6,7}\b"), "<telefono>"),
    # percorsi: tutto il percorso diventa un segnaposto, il nome del file resta (dice "dove" senza dire di chi)
    # avido fino all'ultima barra della riga: i nomi di cartella con spazi (persone, clienti) spariscono tutti
    (re.compile(r"(~|/Users|/home|/private|/var|/opt|/Volumes|[A-Z]:\\)[^\n'\"`]*[/\\]([^/\\\s'\"`)]+)"), r"<percorso>/\2"),
    (re.compile(r"https?://\S+"), "<link>"),
]


# Argomenti che non partono mai, nemmeno filtrati: indizi di password e credenziali non hanno una forma
# riconoscibile (numeri, nomi di familiari), quindi si salta l'intero prompt.
SENSIBILE = re.compile(r"(passw|\bpwd\b|password|credenzial|\bpin\b|\bpuk\b|seed phrase|frase segreta|codice (di accesso|segreto)|chiave privata|private key|\botp\b)", re.I)


def sensibile(testo: str) -> bool:
    return bool(SENSIBILE.search(testo))


def filtra(testo: str) -> str:
    for rx, sost in FILTRI:
        testo = rx.sub(sost, testo)
    return testo


def chiave() -> str:
    k = os.environ.get("TYPESAFE_API_KEY")
    if k:
        return k
    out = subprocess.run(["security", "find-generic-password", "-s", "TYPESAFE_API_KEY", "-w"], capture_output=True, text=True)
    if out.returncode != 0:
        raise SystemExit("Chiave TypeSafe non trovata (TYPESAFE_API_KEY o Portachiavi).")
    return out.stdout.strip()


def stato(prompt: str, prima: str | None) -> dict:
    s = {
        "setting": "A person is typing a message to an AI coding agent (such as Claude Code or Codex) that works inside a project folder and can read its files and run commands.",
        "request": filtra(prompt.strip())[:6000],
    }
    if prima:
        s["agent_last_message"] = filtra(prima.strip())[-1500:]
    return s


def corpo(prompt: str, prima: str | None = None) -> dict:
    return {"model": MODELLO, "state": stato(prompt, prima), "questions": DOMANDE}


def chiedi(prompt: str, prima: str | None = None, k: str | None = None, tentativi: int = 3) -> dict:
    # Prudenza: gli indizi di una password possono non contenere parole rivelatrici e si riconoscono
    # solo dal messaggio dell'agente che li chiedeva. Se l'argomento compare in uno dei due, non parte niente.
    if sensibile(prompt) or (prima and sensibile(prima)):
        raise PermissionError("argomento sensibile (password o credenziali): non parte")
    dati = json.dumps(corpo(prompt, prima)).encode()
    for i in range(tentativi):
        req = urllib.request.Request(URL, data=dati, method="POST",
                                     headers={"Authorization": f"Bearer {k or chiave()}", "Content-Type": "application/json"})
        try:
            t0 = time.time()
            with urllib.request.urlopen(req, timeout=60) as r:
                risposta = json.loads(r.read())
            risposta["_secondi"] = round(time.time() - t0, 2)
            return risposta
        except urllib.error.HTTPError as e:
            if e.code in (429, 500, 502, 503, 504) and i < tentativi - 1:
                time.sleep(2 ** i)
                continue
            raise RuntimeError(f"TypeSafe ha risposto {e.code}: {e.read()[:300]!r}")
    raise RuntimeError("TypeSafe non risponde")


def valuta(risposta: dict) -> dict:
    """Probabilità → esito per punto: 'si' / 'no' / 'incerto'. Se non è un compito, i punti non si applicano."""
    p = {k: v.get("noul") for k, v in risposta.get("answers", {}).items()}
    def esito(x):
        return "si" if x is not None and x >= SI else "no" if x is not None and x <= NO else "incerto"
    t = risposta.get("answers", {}).get("tipo", {})
    tipo = t.get("choice")
    compito = tipo == "new_task"
    punti = {k: {"p": p.get(k), "esito": esito(p.get(k))} for k in NOMI} if compito else {}
    return {"tipo": tipo, "fiducia_tipo": t.get("confidence"), "compito": compito, "punti": punti, "modello": risposta.get("model"), "usage": risposta.get("usage")}


def _prova():
    casi = {
        "scrivi a mario.rossi@cliente.it": "<email>",
        "guarda /Users/mario/Library/CloudStorage/Dropbox-Studio/Mario Rossi/archivio/1. gruppo/x/PIANO.md": "<percorso>/PIANO.md",
        "la chiave è sk-abcdefghijklmnop1234": "<chiave>",
        "chiamami al 347 1234567": "<telefono>",
    }
    for testo, atteso in casi.items():
        f = filtra(testo)
        assert atteso in f, (testo, f)
    assert sensibile("non ricordo la password dell'archivio") and sensibile("ho le pwd del server")
    assert not sensibile("sistema il login")
    assert valuta({"answers": {"tipo": {"choice": "approval_or_choice"}}})["punti"] == {}
    v = valuta({"answers": {"tipo": {"choice": "new_task"}, "perche": {"noul": 0.8}, "vincoli": {"noul": 0.5}, "dove": {"noul": 0.1}}})
    assert v["punti"]["perche"]["esito"] == "si" and v["punti"]["vincoli"]["esito"] == "incerto" and v["punti"]["dove"]["esito"] == "no"
    print("prova superata: filtro e soglie")


if __name__ == "__main__":
    a = sys.argv[1:]
    if not a or a[0] == "--prova":
        _prova()
        sys.exit(0)
    if a[0] == "--filtra":
        # per i riassunti di Crew Deck: testo su stdin, testo oscurato su stdout (UTF-8 in ogni caso)
        sys.stdout.buffer.write(filtra(sys.stdin.buffer.read().decode("utf-8", "replace")).encode("utf-8"))
        sys.exit(0)
    prima = None
    if "--prima" in a:
        i = a.index("--prima"); prima = a[i + 1]; del a[i:i + 2]
    if a[0] == "--json":
        # per Crew Deck: una riga JSON, errori compresi, mai eccezioni
        try:
            print(json.dumps(valuta(chiedi(a[1], prima)), ensure_ascii=False))
        except PermissionError:
            print(json.dumps({"saltato": "sensibile"}))
        except Exception as e:
            print(json.dumps({"errore": str(e)[:300]}))
        sys.exit(0)
    if a[0] == "--payload":
        print(json.dumps(corpo(a[1], prima), ensure_ascii=False, indent=2))
        sys.exit(0)
    print(json.dumps(valuta(chiedi(a[0], prima)), ensure_ascii=False, indent=2))
