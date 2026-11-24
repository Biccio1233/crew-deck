#!/usr/bin/env python3
"""Hook UserPromptSubmit di Claude Code: giudica il prompt con Jev, in background, e registra solo numeri.

Non blocca, non scrive nella sessione, non rallenta: legge l'evento, si stacca e restituisce subito.
Registro: ~/.crew-deck/prompt-registro.jsonl (mai il testo del prompt).
Spegnere: creare ~/.crew-deck/giudice-spento, oppure CREWDECK_GIUDICE=0.

Installazione (in ~/.claude/settings.json, hooks.UserPromptSubmit):
    {"type": "command", "command": "python3 <percorso>/giudice/hook.py", "timeout": 5}
"""
from __future__ import annotations

import hashlib
import json
import os
import pathlib
import sys
import time

QUI = pathlib.Path(__file__).resolve().parent
CARTELLA = pathlib.Path.home() / ".crew-deck"
REGISTRO = CARTELLA / "prompt-registro.jsonl"
SPENTO = CARTELLA / "giudice-spento"


def ultimo_messaggio_agente(trascrizione: str | None) -> str | None:
    """Testo dell'ultimo messaggio dell'assistente, leggendo solo la coda della trascrizione."""
    if not trascrizione:
        return None
    try:
        with open(trascrizione, "rb") as f:
            f.seek(0, 2)
            f.seek(max(0, f.tell() - 400_000))
            righe = f.read().decode("utf-8", "replace").splitlines()
    except OSError:
        return None
    for riga in reversed(righe):
        try:
            d = json.loads(riga)
        except ValueError:
            continue
        if d.get("type") != "assistant":
            continue
        parti = (d.get("message") or {}).get("content") or []
        testo = " ".join(p.get("text", "") for p in parti if isinstance(p, dict) and p.get("type") == "text").strip()
        if testo:
            return testo
    return None


def registra(voce: dict) -> None:
    CARTELLA.mkdir(exist_ok=True)
    with open(REGISTRO, "a", encoding="utf-8") as f:
        f.write(json.dumps(voce, ensure_ascii=False) + "\n")


def lavora(evento: dict) -> None:
    sys.path.insert(0, str(QUI))
    import giudice as G

    prompt = (evento.get("prompt") or "").strip()
    cwd = evento.get("cwd") or ""
    voce = {
        "ts": time.strftime("%Y-%m-%dT%H:%M:%S%z"),
        "agente": "claude",
        "sessione": evento.get("session_id"),
        "progetto": "Cartella home" if cwd.rstrip("/") == os.path.expanduser("~").rstrip("/") else (os.path.basename(cwd.rstrip("/")) or None),
        "parole": len(prompt.split()),
        # impronta per riconoscere lo stesso prompt in seguito, senza conservarne il testo
        "impronta": hashlib.sha256(prompt.encode()).hexdigest()[:16],
    }
    if prompt.startswith("<"):
        voce["esito"] = "saltato_sistema"
    elif voce["parole"] <= 3:
        voce["esito"] = "saltato_breve"
    else:
        prima = ultimo_messaggio_agente(evento.get("transcript_path"))
        try:
            v = G.valuta(G.chiedi(prompt, prima))
            voce.update({
                "esito": "giudicato",
                "tipo": v["tipo"],
                "fiducia_tipo": v["fiducia_tipo"],
                "punti": {k: round(x["p"], 3) for k, x in v["punti"].items() if x["p"] is not None},
                "modello": v["modello"],
                "token": v["usage"],
            })
        except PermissionError:
            voce["esito"] = "saltato_sensibile"
        except Exception as e:  # mai rumore nella sessione: l'errore resta nel registro
            voce["esito"] = "errore"
            voce["errore"] = str(e)[:200]
    registra(voce)


def main() -> None:
    if os.environ.get("CREWDECK_GIUDICE") == "0" or SPENTO.exists():
        return
    try:
        evento = json.load(sys.stdin)
    except ValueError:
        return
    if "--subito" in sys.argv:  # per le prove: nessun distacco
        lavora(evento)
        return
    # ci si stacca: il padre esce subito e Claude Code prosegue senza attendere
    if os.fork() > 0:
        return
    os.setsid()
    devnull = os.open(os.devnull, os.O_RDWR)
    for fd in (0, 1, 2):
        os.dup2(devnull, fd)
    try:
        lavora(evento)
    finally:
        os._exit(0)


if __name__ == "__main__":
    main()
