<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { onMount } from "svelte";

  type Stato = "aspetta" | "lavora" | "finito" | "inattivo";
  type Vista = {
    id: string; agente: "claude" | "codex" | "pi"; cartella: string | null; progetto: string;
    titolo: string | null; modello: string | null; richiesta: string | null; n_richieste: number;
    ultima_attivita: number; ruolo: string; costo_usd: number | null; errori: number;
    automatica: boolean; origine: string | null;
    token: { input: number; output: number; cache_lettura: number; cache_scrittura: number };
    stato: Stato; motivo: string; stimato: boolean; da_quando: number; azione: string;
  };

  const STATI: Record<Stato, { icona: string; nome: string }> = {
    aspetta: { icona: "🚩", nome: "Chiama lo skipper" },
    lavora: { icona: "⛵", nome: "In navigazione" },
    finito: { icona: "🏁", nome: "Al traguardo" },
    inattivo: { icona: "⚓", nome: "In porto" },
  };
  const RUOLI: Record<string, string> = {
    tattico: "Tattico", navigatore: "Navigatore", prodiere: "Prodiere",
    grinder: "Grinder", randista: "Randista", timoniere: "Timoniere",
  };
  const ORDINE: Stato[] = ["aspetta", "lavora", "finito", "inattivo"];

  let sessioni = $state<Vista[]>([]);
  let mostraAutomatiche = $state(false);
  let mostraInPorto = $state(false);
  let ora = $state(Date.now());

  const visibili = $derived(
    sessioni.filter((s) => (mostraAutomatiche || !s.automatica) && (mostraInPorto || s.stato !== "inattivo"))
  );
  const conteggi = $derived(
    Object.fromEntries(ORDINE.map((k) => [k, sessioni.filter((s) => !s.automatica && s.stato === k).length]))
  );
  const inMovimento = $derived(sessioni.some((s) => !s.automatica && s.stato === "lavora"));
  const costoOggi = $derived(
    sessioni.filter((s) => s.ultima_attivita > Date.now() - 86_400_000).reduce((t, s) => t + (s.costo_usd ?? 0), 0)
  );

  function trascorso(ms: number) {
    const s = Math.max(0, Math.round((ora - ms) / 1000));
    if (s < 60) return `${s}s`;
    if (s < 3600) return `${Math.floor(s / 60)}m`;
    if (s < 86400) return `${Math.floor(s / 3600)}h ${Math.floor((s % 3600) / 60)}m`;
    return `${Math.floor(s / 86400)}g`;
  }
  function token(s: Vista) {
    const t = s.token.input + s.token.output + s.token.cache_lettura + s.token.cache_scrittura;
    return t >= 1e6 ? `${(t / 1e6).toFixed(1)}M` : t >= 1e3 ? `${Math.round(t / 1e3)}k` : `${t}`;
  }

  onMount(() => {
    invoke<Vista[]>("sessioni").then((v) => (sessioni = v));
    const stop = listen<Vista[]>("sessioni", (e) => (sessioni = e.payload));
    const t = setInterval(() => (ora = Date.now()), 1000);
    return () => { stop.then((f) => f()); clearInterval(t); };
  });
</script>

<main>
  <header>
    <div class="marchio">
      <svg class="barca" class:naviga={inMovimento} viewBox="0 0 64 48" aria-hidden="true">
        <path class="onda" d="M0 42 Q8 38 16 42 T32 42 T48 42 T64 42" />
        <path class="scafo" d="M10 34 H54 L48 41 H16 Z" />
        <path class="vela" d="M32 6 V31 H14 Z" />
        <path class="fiocco" d="M34 10 V31 H48 Z" />
      </svg>
      <div>
        <h1>Crew Deck</h1>
        <p>{inMovimento ? "L'equipaggio è in navigazione" : "Tutti in porto o al traguardo"}</p>
      </div>
    </div>
    <div class="riassunto">
      {#each ORDINE as k}
        <span class="pill {k}" title={STATI[k].nome}>{STATI[k].icona} {conteggi[k]}</span>
      {/each}
      <span class="pill costo" title="Costo noto delle ultime 24 ore">${costoOggi.toFixed(2)}</span>
    </div>
  </header>

  <nav>
    <label><input type="checkbox" bind:checked={mostraInPorto} /> Mostra in porto</label>
    <label><input type="checkbox" bind:checked={mostraAutomatiche} /> Mostra sessioni automatiche</label>
  </nav>

  {#if visibili.length === 0}
    <p class="vuoto">Nessuna sessione attiva. L'equipaggio riposa.</p>
  {/if}

  <ul>
    {#each visibili as s (s.agente + s.id)}
      <li class={s.stato}>
        <div class="stato" title={STATI[s.stato].nome + (s.stimato ? " (stima)" : "")}>{STATI[s.stato].icona}</div>
        <div class="corpo">
          <div class="riga1">
            <strong>{s.progetto}</strong>
            <span class="agente {s.agente}">{s.agente}</span>
            <span class="ruolo">{RUOLI[s.ruolo] ?? s.ruolo}</span>
            {#if s.automatica}<span class="auto">automatica</span>{/if}
            <span class="tempo">{trascorso(s.da_quando)}</span>
          </div>
          <div class="azione">{s.azione}{s.stimato ? " · stima" : ""}</div>
          {#if s.richiesta}<div class="richiesta" title={s.richiesta}>«{s.richiesta}»</div>{/if}
          <div class="meta">
            <span>{s.n_richieste} richieste</span>
            <span>{token(s)} token</span>
            {#if s.costo_usd != null}<span>${s.costo_usd.toFixed(2)}</span>{/if}
            {#if s.modello}<span>{s.modello}</span>{/if}
            {#if s.errori}<span class="err">{s.errori} errori</span>{/if}
            <span class="cartella" title={s.cartella ?? ""}>{s.cartella ?? ""}</span>
          </div>
        </div>
      </li>
    {/each}
  </ul>
</main>

<style>
  :global(body) {
    margin: 0;
    font-family: Inter, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
    background: radial-gradient(1200px 600px at 20% -10%, #123a5c 0%, #0a1726 55%, #060d16 100%);
    color: #e8f1f8;
    min-height: 100vh;
  }
  main { padding: 20px 24px 40px; max-width: 980px; margin: 0 auto; }
  header { display: flex; justify-content: space-between; align-items: center; gap: 16px; flex-wrap: wrap; }
  .marchio { display: flex; align-items: center; gap: 14px; }
  h1 { margin: 0; font-size: 22px; letter-spacing: 0.04em; text-transform: uppercase; }
  .marchio p { margin: 2px 0 0; color: #8fb3cf; font-size: 13px; }
  .barca { width: 64px; height: 48px; }
  .onda { fill: none; stroke: #3fa7d6; stroke-width: 2; opacity: 0.7; }
  .scafo { fill: #f4f1e8; }
  .vela { fill: #ff6b4a; transform-origin: 32px 31px; }
  .fiocco { fill: #ffd166; transform-origin: 34px 31px; }
  .barca.naviga { animation: beccheggio 2.4s ease-in-out infinite; }
  .barca.naviga .vela { animation: gonfia 1.8s ease-in-out infinite; }
  .barca.naviga .onda { animation: onde 1.2s linear infinite; }
  .barca:not(.naviga) .vela, .barca:not(.naviga) .fiocco { transform: scaleY(0.35); opacity: 0.6; transition: all 0.8s; }
  @keyframes beccheggio { 0%,100% { transform: rotate(-3deg) translateY(0); } 50% { transform: rotate(3deg) translateY(-2px); } }
  @keyframes gonfia { 0%,100% { transform: skewX(0deg); } 50% { transform: skewX(-6deg); } }
  @keyframes onde { from { stroke-dashoffset: 0; } to { stroke-dashoffset: -16; } }
  .onda { stroke-dasharray: 10 6; }

  .riassunto { display: flex; gap: 8px; flex-wrap: wrap; }
  .pill { padding: 6px 12px; border-radius: 999px; background: #ffffff10; font-size: 14px; font-variant-numeric: tabular-nums; }
  .pill.aspetta { background: #ff6b4a33; }
  .pill.lavora { background: #3fa7d633; }
  .pill.costo { background: #ffd16622; color: #ffd166; }

  nav { display: flex; gap: 18px; margin: 18px 0 10px; color: #8fb3cf; font-size: 13px; }
  nav input { accent-color: #3fa7d6; }
  .vuoto { color: #8fb3cf; text-align: center; margin-top: 60px; }

  ul { list-style: none; padding: 0; margin: 0; display: grid; gap: 10px; }
  li {
    display: flex; gap: 14px; padding: 14px 16px; border-radius: 14px;
    background: #ffffff0a; border: 1px solid #ffffff12; border-left: 4px solid #ffffff22;
  }
  li.aspetta { border-left-color: #ff6b4a; background: #ff6b4a14; }
  li.lavora { border-left-color: #3fa7d6; }
  li.finito { border-left-color: #6fd08c; }
  li.inattivo { opacity: 0.6; }
  .stato { font-size: 24px; line-height: 1; padding-top: 2px; }
  .corpo { flex: 1; min-width: 0; }
  .riga1 { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; }
  .riga1 strong { font-size: 15px; }
  .agente, .ruolo, .auto { font-size: 11px; padding: 2px 8px; border-radius: 6px; text-transform: uppercase; letter-spacing: 0.05em; }
  .agente.claude { background: #d9775733; color: #f2a98c; }
  .agente.codex { background: #ffffff1c; color: #d8e3ec; }
  .agente.pi { background: #9b8cff33; color: #c4baff; }
  .ruolo { background: #3fa7d622; color: #9fd6ef; }
  .auto { background: #ffffff10; color: #8fb3cf; }
  .tempo { margin-left: auto; color: #8fb3cf; font-size: 12px; font-variant-numeric: tabular-nums; }
  .azione { margin-top: 6px; font-size: 14px; }
  .richiesta { margin-top: 4px; font-size: 13px; color: #b9cfdf; font-style: italic; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .meta { margin-top: 8px; display: flex; gap: 12px; flex-wrap: wrap; font-size: 12px; color: #7d9cb5; }
  .meta .err { color: #ff8a70; }
  .cartella { white-space: nowrap; overflow: hidden; text-overflow: ellipsis; max-width: 360px; }
</style>
