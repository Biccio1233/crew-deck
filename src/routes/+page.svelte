<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { onMount } from "svelte";
  import Icona from "$lib/Icona.svelte";
  import Carteggio from "$lib/Carteggio.svelte";
  import Debriefing from "$lib/Debriefing.svelte";

  type Stato = "aspetta" | "lavora" | "finito" | "inattivo";
  type Vista = {
    id: string; agente: "claude" | "codex" | "pi"; cartella: string | null; progetto: string;
    titolo: string | null; modello: string | null; richiesta: string | null; n_richieste: number;
    inizio: number | null; ultima_attivita: number; ruolo: string; costo_usd: number | null; errori: number;
    automatica: boolean; origine: string | null;
    token: { input: number; output: number; cache_lettura: number; cache_scrittura: number };
    stato: Stato; motivo: string; stimato: boolean; da_quando: number; azione: string;
    file: string; ultima_app: string | null; ultima_risposta: string | null;
    posizione: { app: string; pid: number; tty: string | null; cmux_surface: string | null; incerta: boolean } | null;
  };

  const SEZIONI: { stato: Stato; titolo: string; filtro: string }[] = [
    { stato: "aspetta", titolo: "Ti aspettano", filtro: "Ti aspettano" },
    { stato: "lavora", titolo: "In navigazione", filtro: "Al lavoro" },
    { stato: "finito", titolo: "Al traguardo", filtro: "Finiti" },
    { stato: "inattivo", titolo: "In porto", filtro: "In porto" },
  ];
  const RUOLI: Record<string, string> = {
    tattico: "Tattico", navigatore: "Navigatore", prodiere: "Prodiere",
    grinder: "Grinder", randista: "Randista", timoniere: "Timoniere",
  };
  const AGENTI: Record<string, string> = { claude: "Claude Code", codex: "Codex", pi: "pi" };

  let sessioni = $state<Vista[]>([]);
  let vista = $state<"plancia" | "carteggio" | "debriefing">("plancia");
  let destinazione = $state<string | null>(null);
  function scriviA(s: Vista) { destinazione = `${s.agente}:${s.id}`; vista = "carteggio"; }
  let filtro = $state<Stato | "attivi">("attivi");
  let automatiche = $state(false);
  let aperta = $state<string | null>(null);
  let ora = $state(Date.now());
  let caricato = $state(false);
  let espansi = $state<Record<string, boolean>>({});
  const LIMITE = 5;
  const compatta = (st: Stato) => st === "finito" || st === "inattivo";

  const umane = $derived(sessioni.filter((s) => automatiche || !s.automatica));
  const conta = $derived(
    Object.fromEntries(SEZIONI.map(({ stato }) => [stato, umane.filter((s) => s.stato === stato).length])) as Record<Stato, number>
  );
  const sezioni = $derived(
    SEZIONI.filter(({ stato }) => (filtro === "attivi" ? stato !== "inattivo" : filtro === stato))
      .map((sez) => ({ ...sez, righe: umane.filter((s) => s.stato === sez.stato) }))
      .filter((sez) => sez.righe.length > 0)
  );
  const naviga = $derived(conta.lavora > 0);
  const costoOggi = $derived(
    umane.filter((s) => s.ultima_attivita > ora - 86_400_000).reduce((t, s) => t + (s.costo_usd ?? 0), 0)
  );

  function trascorso(ms: number) {
    const s = Math.max(0, Math.round((ora - ms) / 1000));
    if (s < 60) return `${s} s`;
    if (s < 3600) return `${Math.floor(s / 60)} min`;
    if (s < 86400) return `${Math.floor(s / 3600)} h ${String(Math.floor((s % 3600) / 60)).padStart(2, "0")}`;
    return `${Math.floor(s / 86400)} g`;
  }
  function token(s: Vista) {
    const t = s.token.input + s.token.output + s.token.cache_lettura + s.token.cache_scrittura;
    return t >= 1e6 ? `${(t / 1e6).toFixed(1)} M` : t >= 1e3 ? `${Math.round(t / 1e3)} k` : `${t}`;
  }
  const chiave = (s: Vista) => s.agente + ":" + s.id;
  const NOMI_APP: Record<string, string> = { Terminal: "Terminale", Code: "VS Code", Claude: "app Claude", Codex: "app Codex" };
  const nomeApp = (a: string) => NOMI_APP[a] ?? a;
  const appDesktop = (s: Vista) =>
    s.agente === "claude" && s.origine === "claude-desktop" ? "app Claude"
    : s.agente === "codex" && (s.origine ?? "").startsWith("Codex Desktop") ? "app Codex" : null;
  const etichettaAzione = (s: Vista) =>
    appDesktop(s) ? `Apri nell'${appDesktop(s)}` : s.posizione ? `Apri in ${nomeApp(s.posizione.app)}` : "Riprendi";

  let esito = $state<{ testo: string; errore: boolean } | null>(null);
  let inCorso = $state<string | null>(null);
  let timerEsito: ReturnType<typeof setTimeout> | undefined;

  async function apri(s: Vista) {
    inCorso = chiave(s);
    try {
      const testo = await invoke<string>("apri_sessione", { agente: s.agente, id: s.id });
      esito = { testo, errore: false };
    } catch (e) {
      esito = { testo: String(e), errore: true };
    } finally {
      inCorso = null;
      clearTimeout(timerEsito);
      timerEsito = setTimeout(() => (esito = null), 5000);
    }
  }

  onMount(() => {
    // anteprima nel browser (senza Tauri): legge un file di esempio
    if (!("__TAURI_INTERNALS__" in window)) {
      fetch("/anteprima.json").then((r) => r.json()).then((v) => { sessioni = v; caricato = true; }).catch(() => (caricato = true));
      const t = setInterval(() => (ora = Date.now()), 1000);
      return () => clearInterval(t);
    }
    invoke<Vista[]>("sessioni").then((v) => { sessioni = v; caricato = true; }).catch(() => (caricato = true));
    const stop = listen<Vista[]>("sessioni", (e) => { sessioni = e.payload; caricato = true; });
    const t = setInterval(() => (ora = Date.now()), 1000);
    return () => { stop.then((f) => f()); clearInterval(t); };
  });
</script>

<div class="app">
  <header class="barra">
    <div class="marchio">
      <svg class="barca" class:naviga viewBox="0 0 28 22" aria-hidden="true">
        <path class="vela" d="M13 2v13H5z" />
        <path class="fiocco" d="M14.6 4.5V15h6z" />
        <path class="scafo" d="M3 16.5h22l-2.6 3.2H5.6z" />
      </svg>
      <span class="nome">Crew Deck</span>
    </div>

    <nav class="sezioni" aria-label="Sezioni">
      <button aria-current={vista === "plancia"} onclick={() => (vista = "plancia")}>Plancia{#if conta.aspetta}<span class="pallino" title="Qualcuno ti aspetta"></span>{/if}</button>
      <button aria-current={vista === "carteggio"} onclick={() => (vista = "carteggio")}>Carteggio</button>
      <button aria-current={vista === "debriefing"} onclick={() => (vista = "debriefing")}>Debriefing</button>
    </nav>

    {#if vista === "plancia"}
    <div class="filtri" role="tablist" aria-label="Filtra per stato">
      <button role="tab" aria-selected={filtro === "attivi"} onclick={() => (filtro = "attivi")}>
        Attivi <span class="n">{conta.aspetta + conta.lavora + conta.finito}</span>
      </button>
      {#each SEZIONI as sez}
        <button role="tab" class={sez.stato} aria-selected={filtro === sez.stato} onclick={() => (filtro = sez.stato)}>
          {sez.filtro} <span class="n">{conta[sez.stato]}</span>
        </button>
      {/each}
    </div>

    {/if}

    <div class="destra">
      <span class="costo" title="Costo noto nelle ultime 24 ore">oggi <strong>${costoOggi.toFixed(2)}</strong></span>
      {#if vista === "plancia"}
      <button class="interruttore" aria-pressed={automatiche} onclick={() => (automatiche = !automatiche)}
        title="Sessioni avviate da altri programmi (graphify, Floorplan, script)">Automatiche</button>
      {/if}
    </div>
  </header>

  {#if vista === "carteggio"}
    <Carteggio sessioni={umane} bind:destinazione />
  {:else if vista === "debriefing"}
    <Debriefing />
  {:else}
  <main>
    {#if !caricato}
      <div class="scheletro">{#each [1, 2, 3] as _}<div class="riga-s"></div>{/each}</div>
    {:else if sezioni.length === 0}
      <div class="vuoto">
        <svg viewBox="0 0 28 22" aria-hidden="true"><path class="vela" d="M13 2v13H5z" /><path class="scafo" d="M3 16.5h22l-2.6 3.2H5.6z" /></svg>
        <p class="t">Nessuno in mare</p>
        <p>Quando apri una sessione di Claude Code, Codex o pi, compare qui da sola.</p>
      </div>
    {/if}

    {#each sezioni as sez (sez.stato)}
      <section class={sez.stato}>
        <h2>{sez.titolo} <span>{sez.righe.length}</span></h2>
        <ul>
          {#each (espansi[sez.stato] || filtro === sez.stato ? sez.righe : sez.righe.slice(0, LIMITE)) as s (chiave(s))}
            <li class:aperta={aperta === chiave(s)} class:compatta={compatta(s.stato) && aperta !== chiave(s)}>
              <button class="riga" onclick={() => (aperta = aperta === chiave(s) ? null : chiave(s))} aria-expanded={aperta === chiave(s)}>
                <span class="ico"><Icona stato={s.stato} /></span>
                <span class="principale">
                  <span class="r1">
                    <span class="progetto">{s.progetto}</span>
                    {#if s.titolo}<span class="titolo">{s.titolo}</span>{/if}
                  </span>
                  <span class="azione">
                    {s.azione}{#if s.stimato}<span class="stima">stima</span>{/if}
                  </span>
                  {#if s.richiesta}<span class="richiesta">{s.richiesta}</span>{/if}
                </span>
                <span class="lato">
                  <span class="tempo">{trascorso(s.da_quando)}{#if s.posizione}<span class="dove" title={s.posizione.tty ?? ""}>{" · "}{nomeApp(s.posizione.app)}</span>{/if}</span>
                  <span class="etichette">
                    <span class="agente {s.agente}">{AGENTI[s.agente]}</span>
                    {#if appDesktop(s)}<span class="ruolo desk">app</span>{/if}
                    <span class="ruolo">{RUOLI[s.ruolo] ?? s.ruolo}</span>
                  </span>
                  <span class="numeri">{token(s)} tok{#if s.costo_usd != null}{" · "}${s.costo_usd.toFixed(2)}{/if}</span>
                </span>
              </button>
              <button class="vai" class:viva={!!s.posizione || !!appDesktop(s)} disabled={inCorso === chiave(s) || (!s.posizione && !appDesktop(s) && !s.cartella)}
                onclick={() => apri(s)} title={s.posizione ? "Porta in primo piano la scheda dove gira" : "Apre una scheda nuova nella cartella e riprende la sessione"}>
                {#if s.posizione || appDesktop(s)}
                  <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M6 3h7v7M13 3L6.5 9.5M11 9.5V13H3V5h3.5" /></svg>
                {:else}
                  <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M3 8a5 5 0 1 0 1.5-3.5M3 2.5V5h2.5" /></svg>
                {/if}
                {inCorso === chiave(s) ? "…" : etichettaAzione(s)}
              </button>
              {#if aperta === chiave(s)}
                <dl class="dettagli">
                  <div><dt>Cartella</dt><dd class="mono">{s.cartella ?? "sconosciuta"}</dd></div>
                  <div class="largo"><dt>Ultima richiesta</dt><dd class="clamp">{s.richiesta ?? "nessuna"}</dd></div>
                  {#if s.ultima_risposta}<div class="largo"><dt>Ultima risposta</dt><dd class="clamp">{s.ultima_risposta.replace(/\*\*(.+?)\*\*/g, "$1").replace(/`([^`]+)`/g, "$1")}</dd></div>{/if}
                  <div><dt>Modello</dt><dd>{s.modello ?? "?"}</dd></div>
                  <div><dt>Richieste</dt><dd>{s.n_richieste}</dd></div>
                  <div><dt>Token</dt><dd class="mono">in {s.token.input} · out {s.token.output} · cache {s.token.cache_lettura}/{s.token.cache_scrittura}</dd></div>
                  {#if s.errori}<div><dt>Errori</dt><dd class="err">{s.errori}</dd></div>{/if}
                  <div><dt>Origine</dt><dd>{s.origine ?? AGENTI[s.agente]}{s.automatica ? " · automatica" : ""}</dd></div>
                  <div><dt>Dove gira</dt><dd>{s.posizione ? `${nomeApp(s.posizione.app)}${s.posizione.tty ? " · " + s.posizione.tty.replace("/dev/", "") : ""}${s.posizione.incerta ? " · collegamento stimato" : ""}` : s.ultima_app ? `chiusa (ultima volta in ${nomeApp(s.ultima_app)})` : "chiusa"}</dd></div>
                  <div><dt>Sessione</dt><dd class="mono">{s.id}</dd></div>
                  {#if s.posizione}<div><dt>Prossima richiesta</dt><dd><button class="link" onclick={() => scriviA(s)}>Scrivi a questa sessione →</button></dd></div>{/if}
                </dl>
              {/if}
            </li>
          {/each}
        </ul>
        {#if sez.righe.length > LIMITE && filtro !== sez.stato}
          <button class="altri" onclick={() => (espansi[sez.stato] = !espansi[sez.stato])}>
            {espansi[sez.stato] ? "Mostra meno" : `Mostra altre ${sez.righe.length - LIMITE}`}
          </button>
        {/if}
      </section>
    {/each}
  </main>
  {/if}

  {#if esito}
    <div class="esito" class:errore={esito.errore} role="status">{esito.testo}</div>
  {/if}
</div>

<style>
  :global(:root) {
    color-scheme: dark light;
    /* grafite calda, tinta verso la tela delle vele */
    --bg: oklch(0.175 0.006 70);
    --pannello: oklch(0.205 0.007 70);
    --riga-hover: oklch(0.235 0.008 70);
    --bordo: oklch(0.29 0.008 70);
    --testo: oklch(0.94 0.008 80);
    --testo-2: oklch(0.74 0.01 75);
    --testo-3: oklch(0.58 0.01 75);
    --c-aspetta: oklch(0.7 0.18 38);
    --c-lavora: oklch(0.8 0.1 210);
    --c-finito: oklch(0.79 0.13 152);
    --c-inattivo: oklch(0.6 0.01 75);
    --tinta-aspetta: oklch(0.7 0.18 38 / 0.1);
    --tela: oklch(0.95 0.02 85);
    --raggio: 8px;
    --mono: ui-monospace, "SF Mono", "JetBrains Mono", Menlo, monospace;
  }
  @media (prefers-color-scheme: light) {
    :global(:root) {
      --bg: oklch(0.975 0.004 80);
      --pannello: oklch(0.955 0.005 80);
      --riga-hover: oklch(0.935 0.006 80);
      --bordo: oklch(0.88 0.006 80);
      --testo: oklch(0.22 0.01 70);
      --testo-2: oklch(0.42 0.01 70);
      --testo-3: oklch(0.55 0.01 70);
      --c-aspetta: oklch(0.6 0.19 36);
      --c-lavora: oklch(0.55 0.11 220);
      --c-finito: oklch(0.56 0.13 152);
      --c-inattivo: oklch(0.6 0.01 70);
      --tinta-aspetta: oklch(0.6 0.19 36 / 0.08);
      --tela: oklch(0.3 0.02 70);
    }
  }
  :global(body) {
    margin: 0; background: var(--bg); color: var(--testo);
    font: 13px/1.45 -apple-system, BlinkMacSystemFont, "Segoe UI", system-ui, sans-serif;
    -webkit-font-smoothing: antialiased;
  }
  :global(button) { font: inherit; color: inherit; }

  .app { min-height: 100vh; display: flex; flex-direction: column; }

  /* barra */
  .barra {
    position: sticky; top: 0; z-index: 2; display: flex; align-items: center; gap: 16px;
    height: 48px; padding: 0 16px; background: var(--pannello); border-bottom: 1px solid var(--bordo);
  }
  .marchio { display: flex; align-items: center; gap: 8px; }
  .nome { font-weight: 600; font-size: 13px; letter-spacing: 0.01em; }
  .barca { width: 22px; height: 18px; }
  .barca .vela { fill: var(--tela); }
  .barca .fiocco { fill: var(--tela); opacity: 0.55; }
  .barca .scafo { fill: var(--c-aspetta); }
  .barca .vela, .barca .fiocco { transform-origin: 13px 15px; transition: transform 400ms cubic-bezier(0.22, 1, 0.36, 1), opacity 400ms; }
  .barca:not(.naviga) .vela, .barca:not(.naviga) .fiocco { transform: scaleY(0.4); opacity: 0.35; }
  .barca.naviga { animation: beccheggio 3s ease-in-out infinite; transform-origin: 14px 20px; }
  @keyframes beccheggio { 0%, 100% { transform: rotate(-2.5deg); } 50% { transform: rotate(2.5deg); } }

  .sezioni { display: flex; gap: 2px; }
  .sezioni button {
    border: 0; background: transparent; color: var(--testo-2); padding: 6px 10px; border-radius: 6px; cursor: pointer;
    position: relative; transition: color 150ms, background 150ms;
  }
  .sezioni button:hover { color: var(--testo); }
  .sezioni button[aria-current="true"] { color: var(--testo); background: var(--riga-hover); }
  .pallino { position: absolute; top: 5px; right: 3px; width: 6px; height: 6px; border-radius: 50%; background: var(--c-aspetta); }
  .link { border: 0; background: transparent; color: var(--c-lavora); padding: 0; cursor: pointer; font: inherit; }
  .link:hover { text-decoration: underline; }
  .filtri { display: flex; gap: 2px; padding: 3px; background: var(--bg); border: 1px solid var(--bordo); border-radius: 9px; }
  .filtri button {
    border: 0; background: transparent; padding: 4px 10px; border-radius: 6px; cursor: pointer;
    color: var(--testo-2); display: flex; gap: 6px; align-items: center; transition: background 150ms, color 150ms;
  }
  .filtri button:hover { color: var(--testo); }
  .filtri button[aria-selected="true"] { background: var(--riga-hover); color: var(--testo); }
  .filtri .n { font-variant-numeric: tabular-nums; color: var(--testo-3); font-size: 12px; }
  .filtri button.aspetta .n { color: var(--c-aspetta); font-weight: 600; }

  .destra { margin-left: auto; display: flex; align-items: center; gap: 12px; }
  .costo { color: var(--testo-3); font-variant-numeric: tabular-nums; }
  .costo strong { color: var(--testo); font-weight: 600; }
  .interruttore {
    border: 1px solid var(--bordo); background: transparent; padding: 4px 10px; border-radius: 6px;
    color: var(--testo-2); cursor: pointer; transition: background 150ms, color 150ms;
  }
  .interruttore:hover { color: var(--testo); }
  .interruttore[aria-pressed="true"] { background: var(--riga-hover); color: var(--testo); }
  button:focus-visible { outline: 2px solid var(--c-lavora); outline-offset: 1px; }

  /* elenco */
  main { padding: 8px 0 32px; }
  section { margin-top: 14px; }
  h2 {
    margin: 0 0 4px; padding: 0 20px; font-size: 11px; font-weight: 600; letter-spacing: 0.06em;
    text-transform: uppercase; color: var(--testo-3); display: flex; gap: 8px;
  }
  h2 span { font-variant-numeric: tabular-nums; }
  section.aspetta h2 { color: var(--c-aspetta); }
  ul { list-style: none; margin: 0; padding: 0; }
  li { border-top: 1px solid var(--bordo); display: grid; grid-template-columns: minmax(0, 1fr) auto; align-items: start; }
  li > .dettagli { grid-column: 1 / -1; }
  .vai {
    margin: 10px 16px 0 0; display: inline-flex; align-items: center; gap: 6px; white-space: nowrap;
    border: 1px solid var(--bordo); background: transparent; color: var(--testo-2); border-radius: 6px;
    padding: 4px 10px; cursor: pointer; font-size: 12px; width: 156px; justify-content: flex-start; transition: background 150ms, color 150ms, border-color 150ms;
  }
  .vai svg { width: 13px; height: 13px; fill: none; stroke: currentColor; stroke-width: 1.5; stroke-linecap: round; stroke-linejoin: round; }
  .vai:hover:not(:disabled) { background: var(--riga-hover); color: var(--testo); }
  .vai.viva { color: var(--testo); }
  section.aspetta .vai.viva { border-color: var(--c-aspetta); color: var(--c-aspetta); }
  section.aspetta .vai.viva:hover { background: var(--tinta-aspetta); }
  .vai:disabled { opacity: 0.45; cursor: default; }
  .dove { color: var(--testo-3); }
  /* righe compatte: chi ha finito o è in porto occupa una riga sola */
  li.compatta .riga { padding-top: 7px; padding-bottom: 7px; align-items: center; }
  li.compatta .principale { flex-direction: row; gap: 10px; align-items: baseline; }
  li.compatta .azione, li.compatta .richiesta, li.compatta .numeri { display: none; }
  li.compatta .lato { grid-auto-flow: column; align-items: center; gap: 8px; }
  li.compatta .vai { margin-top: 6px; }
  .altri {
    margin: 6px 20px 0; border: 0; background: transparent; color: var(--testo-3); cursor: pointer; padding: 4px 0;
    font-size: 12px;
  }
  .altri:hover { color: var(--testo); }
  .esito {
    position: fixed; left: 50%; bottom: 18px; transform: translateX(-50%); max-width: min(560px, calc(100vw - 32px));
    padding: 8px 14px; border-radius: 8px; background: var(--pannello); border: 1px solid var(--bordo);
    color: var(--testo); box-shadow: 0 6px 24px oklch(0 0 0 / 0.25); animation: apri 200ms cubic-bezier(0.22, 1, 0.36, 1);
  }
  .esito.errore { border-color: var(--c-aspetta); }
  li:last-child { border-bottom: 1px solid var(--bordo); }
  section.aspetta li { background: var(--tinta-aspetta); }

  .riga {
    width: 100%; min-width: 0; display: grid; grid-template-columns: 28px minmax(0, 1fr) max-content; grid-template-rows: auto; gap: 12px;
    align-items: start; padding: 10px 12px 10px 16px; border: 0; background: transparent; text-align: left;
    cursor: pointer; transition: background 150ms;
  }
  .riga:hover { background: var(--riga-hover); }
  .ico { display: flex; justify-content: center; padding-top: 1px; }
  .principale { display: flex; flex-direction: column; gap: 2px; min-width: 0; }
  .r1 { display: flex; gap: 8px; align-items: baseline; min-width: 0; }
  .progetto { font-weight: 600; font-size: 13.5px; white-space: nowrap; }
  .titolo { color: var(--testo-3); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .azione { color: var(--testo); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  section.inattivo .azione, section.finito .azione { color: var(--testo-2); }
  .stima {
    margin-left: 8px; font-size: 11px; color: var(--testo-3); border: 1px solid var(--bordo);
    border-radius: 4px; padding: 0 5px;
  }
  .richiesta { color: var(--testo-3); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; font-size: 12.5px; }
  .richiesta::before { content: "↳ "; opacity: 0.7; }

  .lato { display: grid; justify-items: end; align-content: start; gap: 4px; }
  .tempo { font-variant-numeric: tabular-nums; color: var(--testo-2); font-size: 12px; }
  .etichette { display: flex; gap: 4px; align-items: center; flex-wrap: nowrap; height: 20px; }
  .agente, .ruolo {
    font-size: 11px; padding: 1px 7px; border-radius: 4px; border: 1px solid var(--bordo); color: var(--testo-2);
    white-space: nowrap;
  }
  .agente::before { content: ""; display: inline-block; width: 6px; height: 6px; border-radius: 50%; margin-right: 5px; vertical-align: 1px; }
  .agente.claude::before { background: oklch(0.7 0.13 45); }
  .agente.codex::before { background: oklch(0.85 0.01 80); }
  .agente.pi::before { background: oklch(0.72 0.12 290); }
  .numeri { font-size: 11.5px; color: var(--testo-3); font-variant-numeric: tabular-nums; }

  .dettagli {
    margin: 0; padding: 4px 20px 14px 56px; display: grid; grid-template-columns: repeat(auto-fill, minmax(220px, 1fr));
    gap: 8px 24px; animation: apri 200ms cubic-bezier(0.22, 1, 0.36, 1);
  }
  .dettagli div { min-width: 0; }
  .dettagli .largo { grid-column: 1 / -1; }
  .dettagli .clamp { white-space: pre-wrap; display: -webkit-box; -webkit-line-clamp: 4; line-clamp: 4; -webkit-box-orient: vertical; overflow: hidden; }
  .dettagli dt { font-size: 11px; color: var(--testo-3); text-transform: uppercase; letter-spacing: 0.05em; }
  .dettagli dd { margin: 2px 0 0; color: var(--testo-2); overflow-wrap: anywhere; }
  .dettagli .mono { font-family: var(--mono); font-size: 12px; }
  .dettagli .err { color: var(--c-aspetta); }
  @keyframes apri { from { opacity: 0; transform: translateY(-4px); } }

  .vuoto { text-align: center; margin: 18vh auto 0; max-width: 340px; color: var(--testo-3); }
  .vuoto svg { width: 44px; height: 36px; opacity: 0.5; }
  .vuoto .vela { fill: var(--testo-3); }
  .vuoto .scafo { fill: var(--testo-3); }
  .vuoto .t { color: var(--testo); font-weight: 600; font-size: 14px; margin: 10px 0 4px; }
  .vuoto p { margin: 0; }

  .scheletro { padding: 20px; display: grid; gap: 10px; }
  .riga-s { height: 52px; border-radius: var(--raggio); background: var(--pannello); animation: pulsa 1.4s ease-in-out infinite; }
  @keyframes pulsa { 50% { opacity: 0.5; } }

  @media (max-width: 720px) {
    .filtri button:not([aria-selected="true"]):not(.aspetta) { display: none; }
    .costo { display: none; }
  }
  @media (prefers-reduced-motion: reduce) {
    .barca.naviga { animation: none; }
  }
</style>
