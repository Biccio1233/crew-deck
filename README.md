# ⛵ Crew Deck

**See every AI coding agent on your machine at a glance — who is working, who is done, who is waiting for you.**
*La plancia dell'equipaggio: vedi ogni agente AI al lavoro, chi ha finito e chi ti aspetta.*

Crew Deck reads the session logs that Claude Code, Codex and pi already write on disk, and turns them into a live crew board: state, folder, current request, last action, tokens and cost. Local, read-only, no account, no network by default.

| State | |
|---|---|
| 🚩 **Calling the skipper** | waiting for you: a question or a permission request |
| ⛵ **Sailing** | working: running a tool, thinking, writing |
| 🏁 **Finished** | turn done, interrupted, or API error |
| ⚓ **In harbour** | idle for 30 minutes |

Each agent gets a crew role from what it is doing right now: *Tactician* (planning), *Navigator* (web research), *Bowman* (exploring code), *Grinder* (editing files), *Trimmer* (running commands), *Helmsman* (coordinating sub-agents).

## Status
Early development — phase 1 (*Lookout*): readers for Claude Code, Codex and pi, state engine, live list. See [SPEC.md](SPEC.md) (Italian) for the full plan: tower overlay, notifications, actions, prompt debriefing, project-brain health, MCP/CLI harbour and key vault.

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
cargo run --release --bin vedetta              # terminal view of your sessions
```

## Credits
State logic inspired by [AGENT TOWER](https://github.com/Thomas2306/agent-tower) (MIT), rewritten in Rust and extended to several agents.

## License
MIT
