#!/usr/bin/env python3
"""Hook di Claude Code: segnala a Crew Deck che la sessione ti aspetta.

Claude Code scrive una domanda (AskUserQuestion) nel registro della sessione solo dopo la risposta,
quindi Crew Deck non può vederla da lì. Questo hook lascia un file ~/.crew-deck/attese/<sessione>
con «domanda» o «permesso»; Crew Deck lo ignora appena il registro della sessione si muove di nuovo.
Non scrive nella sessione, non blocca, non registra testi.

Installazione (in ~/.claude/settings.json):
    hooks.PreToolUse   matcher "AskUserQuestion"   → python3 <percorso>/giudice/attesa.py domanda
    hooks.Notification matcher "permission_prompt" → python3 <percorso>/giudice/attesa.py permesso
    hooks.Stop                                     → python3 <percorso>/giudice/attesa.py fine
"""
import json
import os
import pathlib
import re
import sys

try:
    motivo = sys.argv[1]
    evento = json.load(sys.stdin)
    sessione = evento.get("session_id", "")
    valida = re.fullmatch(r"[A-Za-z0-9-]{1,100}", sessione)
    if motivo == "fine" and valida:
        # Claude ha finito di rispondere: la sessione non aspetta più
        (pathlib.Path.home() / ".crew-deck" / "attese" / sessione).unlink(missing_ok=True)
    elif motivo in ("domanda", "permesso") and valida:
        cartella = pathlib.Path.home() / ".crew-deck" / "attese"
        cartella.mkdir(parents=True, exist_ok=True)
        segnale = cartella / sessione
        # la domanda aperta fa partire anche l'avviso «permission_prompt»: non deve sostituirla
        try:
            domanda_aperta = segnale.read_text() == "domanda" and os.path.getmtime(evento["transcript_path"]) <= segnale.stat().st_mtime + 2
        except (OSError, KeyError, TypeError):
            domanda_aperta = False
        if not (motivo == "permesso" and domanda_aperta):
            segnale.write_text(motivo)
except Exception:
    pass  # un hook rotto non deve mai disturbare la sessione
