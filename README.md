# ⛵ Crew Deck

**See every AI coding agent on your machine at a glance — who is working, who is done, who is waiting for you.**
*La plancia dell'equipaggio: vedi ogni agente AI al lavoro, chi ha finito e chi ti aspetta.*

Crew Deck reads the session logs that Claude Code, Codex and pi already write on disk, and turns them into a live crew board: state, folder, current request, last action, tokens and cost. Local, read-only on the agents' logs, no account. Nothing leaves your machine unless you switch on a feature that needs it (see [Privacy](#privacy)).

| State | |
|---|---|
| 🚩 **Calling the skipper** | waiting for you: a question or a permission request |
| ⛵ **Sailing** | working: running a tool, thinking, writing |
| 🏁 **Finished** | turn done, interrupted, or API error |
| ⚓ **In harbour** | idle for 30 minutes |

Each agent gets a crew role from what it is doing right now: *Tactician* (planning), *Navigator* (web research), *Bowman* (exploring code), *Grinder* (editing files), *Trimmer* (running commands), *Helmsman* (coordinating sub-agents).

## Notifications and the one-line summary
Crew Deck sends a macOS notification when a session needs you or finishes something long, and clicking it brings Crew Deck to the front:
- a question from the agent, always;
- a permission request: certain ones (from the optional Claude Code hook) at most once per session per turn; for Codex and pi the request is only estimated, so it notifies after 30 seconds of standing still, at most once per turn;
- a turn that finished after more than 2 minutes, or stopped for a service error.

There is no notification at app start, and none for automatic or closed sessions. Short turns stay quiet.

Each row also shows *su cosa* (what the session is about, up to 8 words) and *esito* (the outcome, or what it is waiting for, up to 12 words). With the **Riassunti automatici** switch on, these lines are written by Claude Haiku through your own `claude` CLI, incrementally: the previous summary plus the last request and the end of the last agent reply. Summaries are stored in `~/.crew-deck/riassunti/`. With the switch off, the row falls back to the session title or the last request.

## Status
Early development, test version 0.5: readers for Claude Code, Codex and pi, state engine, live board, notifications, per-row summaries, and the prompt tools (check, rewrite, suggested replies). See [SPEC.md](SPEC.md) (Italian) for the full plan: tower overlay, actions, project-brain health, MCP/CLI harbour and key vault.

## Privacy
Reading your sessions is local and read-only. Text leaves your machine only in these cases:

| Feature | What is sent, and where | When |
|---|---|---|
| **Automatic summaries** | the previous summary, your last request and the end of the last agent reply, to Claude Haiku via your own `claude -p` (no tools, no saved session) | only if you accept at first launch (default: No); switch **Riassunti automatici** in the header turns it off |
| **Prompt check («Jev»)** | your prompt and the agent's last message, to TypeSafe (`api.typesafe.ai`); emails, API keys, IBANs, tax codes, phone numbers, file paths and links are replaced first | only if you put your own TypeSafe key in the macOS Keychain; without it the check is off |
| **Rifinisci, Approfondisci, Cosa rispondere** | your prompt and the end of the agent's reply, to Claude Sonnet via your own `claude -p` | only when you click the button |

Text that mentions passwords or credentials (password, PIN, PUK, OTP, private key, seed phrase and similar words) is never sent to Jev or to the summaries. The three buttons run on a click you make, so they do not apply that block. Nothing else touches the network.

## Supported agents
| Agent | Log location |
|---|---|
| Claude Code (CLI and desktop app, Code mode) | `~/.claude/projects/` |
| Codex (CLI and desktop app) | `~/.codex/sessions/` |
| pi | `~/.pi/agent/sessions/` |

Regular chats in the Claude or ChatGPT apps live on their servers and cannot be read locally.

## Develop
Requirements: Node 20+, Rust (stable), and the [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/).
```bash
npm install
npm run tauri dev                              # the app
cd src-tauri && cargo test                     # state-engine tests
cargo run --release --example vedetta          # terminal view of your sessions
```

## Credits
State logic inspired by [AGENT TOWER](https://github.com/Thomas2306/agent-tower) (MIT), rewritten in Rust and extended to several agents.
The method of the incremental summaries (topic and outcome format, current goal that follows pivots, open objectives, each summary built on the previous one) is adapted from [billion-context](https://github.com/ranxianglei/billion-context) (MIT).

## License
MIT
