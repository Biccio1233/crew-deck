<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { onMount } from "svelte";

  type Sessione = {
    id: string; agente: "claude" | "codex" | "pi"; progetto: string; cartella: string | null; titolo: string | null;
    stato: string; posizione: { app: string } | null;
  };
  type Punto = { p: number | null; esito: "si" | "no" | "incerto" };
  type Giudizio = { tipo?: string; compito?: boolean; punti?: Record<string, Punto>; saltato?: string; errore?: string };
  type Riscrittura = {
    prompt?: string; domande?: string[]; modifiche?: string[];
    professionista?: string; materiale?: string; metodi?: string; prova?: string; fuori?: string;
  };

  let { sessioni, destinazione = $bindable(null) }: { sessioni: Sessione[]; destinazione: string | null } = $props();

  const TIPI: Record<string, string> = {
    new_task: "Compito nuovo", question: "Domanda", approval_or_choice: "Conferma o scelta",
    answer_to_agent: "Risposta all'agente", comment: "Commento",
  };
  // Avvisi solo dove la taratura li rende affidabili (giudice/TARATURA.md); gli altri sono informazioni.
  const PUNTI: { k: string; nome: string; avviso: boolean; domanda: string; prefisso: string; esempio: string }[] = [
    { k: "criterio_fine", nome: "Come si verifica", avviso: true, domanda: "Come capirai che è fatto?", prefisso: "È fatto quando", esempio: "i test passano, la pagina mostra…" },
    { k: "vincoli", nome: "Vincoli", avviso: true, domanda: "Cosa non deve cambiare, o cosa resta fuori?", prefisso: "Vincoli", esempio: "nessuna nuova libreria, non toccare l'API" },
    { k: "perche", nome: "Perché serve", avviso: false, domanda: "A cosa serve il risultato?", prefisso: "Serve a", esempio: "mandarlo al cliente venerdì" },
    { k: "contesto", nome: "Contesto che l'agente non vede", avviso: false, domanda: "Cosa sai tu che l'agente non può trovare nei file?", prefisso: "Contesto", esempio: "abbiamo deciso di…, l'errore compare quando…" },
  ];

  let testo = $state("");
  let jev = $state(true);
  let giudizio = $state<Giudizio | null>(null);
  let giudicando = $state(false);
  let risposte = $state<Record<string, string>>({});
  let riscrittura = $state<Riscrittura | null>(null);
  let modoRiscrittura = $state<"rifinisci" | "approfondisci" | null>(null);
  let riscrivendo = $state(false);
  let conferma = $state(false);
  let esito = $state<{ testo: string; errore: boolean } | null>(null);
  let agenteNuovo = $state<"claude" | "codex" | "pi">("claude");
  let cartellaNuova = $state("");
  let timer: ReturnType<typeof setTimeout> | undefined;
  let ultimoGiudicato = "";

  const aperte = $derived(sessioni.filter((s) => s.posizione && !s.id.startsWith("_")));
  const cartelle = $derived([...new Set(sessioni.map((s) => s.cartella).filter((c): c is string => !!c))].slice(0, 30));
  const scelta = $derived(destinazione && destinazione !== "nuova" ? sessioni.find((s) => `${s.agente}:${s.id}` === destinazione) ?? null : null);
  const parole = $derived(testo.trim() ? testo.trim().split(/\s+/).length : 0);
  const segnaposti = $derived((testo.match(/\[\[[^\]]+\]\]/g) ?? []).length);
  const avvisi = $derived(
    giudizio?.compito ? PUNTI.filter((p) => p.avviso && giudizio?.punti?.[p.k]?.esito === "no") : []
  );

  function programma() {
    clearTimeout(timer);
    if (!jev || parole <= 3) { giudizio = null; return; }
    timer = setTimeout(giudica, 900);
  }
  const anteprima = typeof window !== "undefined" && !("__TAURI_INTERNALS__" in window);
  async function giudica() {
    if (testo === ultimoGiudicato) return;
    ultimoGiudicato = testo;
    giudicando = true;
    if (anteprima) {
      giudizio = { tipo: "new_task", compito: true, punti: { criterio_fine: { p: 0.08, esito: "no" }, vincoli: { p: 0.12, esito: "no" }, perche: { p: 0.81, esito: "si" }, contesto: { p: 0.77, esito: "si" } } };
      giudicando = false; return;
    }
    try {
      giudizio = await invoke<Giudizio>("giudica_prompt", { testo, agente: scelta?.agente ?? null, id: scelta?.id ?? null });
    } catch (e) {
      giudizio = { errore: String(e) };
    } finally {
      giudicando = false;
    }
  }
  function aggiungi(p: (typeof PUNTI)[number]) {
    const r = (risposte[p.k] ?? "").trim();
    if (!r) return;
    testo = testo.trimEnd() + `\n\n${p.prefisso}: ${r}`;
    risposte[p.k] = "";
    programma();
  }
  async function riscrivi(modo: "rifinisci" | "approfondisci") {
    riscrivendo = true; modoRiscrittura = modo; riscrittura = null;
    const mancanti = giudizio?.compito ? PUNTI.filter((p) => giudizio?.punti?.[p.k]?.esito === "no").map((p) => p.nome) : [];
    try {
      riscrittura = await invoke<Riscrittura>("riscrivi_prompt", { modo, testo, agente: scelta?.agente ?? null, id: scelta?.id ?? null, mancanti });
    } catch (e) {
      mostra(String(e), true);
    } finally {
      riscrivendo = false;
    }
  }
  function usa() {
    if (riscrittura?.prompt) { testo = riscrittura.prompt; riscrittura = null; programma(); }
  }
  function mostra(t: string, errore = false) {
    esito = { testo: t, errore };
    setTimeout(() => (esito = null), 6000);
  }
  async function invia(conInvio: boolean) {
    conferma = false;
    try {
      const msg = destinazione === "nuova"
        ? await invoke<string>("nuova_sessione", { agente: agenteNuovo, cartella: cartellaNuova, testo })
        : await invoke<string>("invia_prompt", { agente: scelta!.agente, id: scelta!.id, testo, invio: conInvio });
      mostra(msg);
    } catch (e) {
      mostra(String(e), true);
    }
  }

  onMount(() => {
    jev = localStorage.getItem("crewdeck.jev") !== "0";
    if (!destinazione) destinazione = aperte[0] ? `${aperte[0].agente}:${aperte[0].id}` : "nuova";
    if (!cartellaNuova && cartelle[0]) cartellaNuova = cartelle[0];
  });
  $effect(() => localStorage.setItem("crewdeck.jev", jev ? "1" : "0"));
</script>

<div class="carteggio">
  <section class="scrittura">
    <div class="riga-dest">
      <label for="dest">A chi</label>
      <select id="dest" bind:value={destinazione} onchange={() => { ultimoGiudicato = ""; programma(); }}>
        {#if aperte.length}
          <optgroup label="Sessioni aperte">
            {#each aperte as s}
              <option value={`${s.agente}:${s.id}`}>{s.progetto} · {s.agente} · {s.posizione?.app}{s.titolo ? ` · ${s.titolo.slice(0, 40)}` : ""}</option>
            {/each}
          </optgroup>
        {/if}
        <option value="nuova">Nuova sessione…</option>
      </select>
      {#if destinazione === "nuova"}
        <select bind:value={agenteNuovo} aria-label="Agente"><option value="claude">Claude Code</option><option value="codex">Codex</option><option value="pi">pi</option></select>
        <select bind:value={cartellaNuova} aria-label="Cartella" class="cartella">
          {#each cartelle as c}<option value={c}>{c.replace(/^\/Users\/[^/]+/, "~")}</option>{/each}
        </select>
      {/if}
    </div>

    <textarea bind:value={testo} oninput={programma} placeholder="Scrivi la richiesta. Il controllo parte mentre scrivi." rows="12" spellcheck="true"></textarea>

    <div class="azioni">
      <button class="sec" disabled={!testo.trim() || riscrivendo} onclick={() => riscrivi("rifinisci")} title="Modifiche minime: aggiunge solo ciò che manca">
        {riscrivendo && modoRiscrittura === "rifinisci" ? "Rifinisco…" : "Rifinisci"}
      </button>
      <button class="sec" disabled={!testo.trim() || riscrivendo} onclick={() => riscrivi("approfondisci")} title="Cosa farebbe un professionista, da quale materiale, con quali metodi, cosa lo dimostrerebbe, cosa resta fuori">
        {riscrivendo && modoRiscrittura === "approfondisci" ? "Approfondisco…" : "Approfondisci"}
      </button>
      <span class="spazio"></span>
      {#if segnaposti}<span class="nota">{segnaposti} {segnaposti === 1 ? "segnaposto da compilare" : "segnaposti da compilare"}</span>{/if}
      <button class="pri" disabled={!testo.trim() || (destinazione !== "nuova" && !scelta) || (destinazione === "nuova" && !cartellaNuova)} onclick={() => (conferma = true)}>
        {destinazione === "nuova" ? "Apri sessione" : "Invia"}
      </button>
    </div>

    {#if conferma}
      <div class="conferma" role="alertdialog" aria-label="Conferma invio">
        {#if destinazione === "nuova"}
          <p>Apro una sessione di <strong>{agenteNuovo}</strong> in <span class="mono">{cartellaNuova.replace(/^\/Users\/[^/]+/, "~")}</span> con questa richiesta?</p>
          <div><button class="pri" onclick={() => invia(true)}>Apri</button><button class="sec" onclick={() => (conferma = false)}>Annulla</button></div>
        {:else}
          <p>Scrivo la richiesta nella scheda di <strong>{scelta?.progetto}</strong> ({scelta?.posizione?.app}){scelta?.stato === "lavora" ? ": l'agente sta lavorando, la riceverà in coda" : ""}.{#if segnaposti} Restano {segnaposti} segnaposti [[…]].{/if}</p>
          <div>
            <button class="pri" onclick={() => invia(true)}>Scrivi e invia</button>
            <button class="sec" onclick={() => invia(false)}>Scrivi senza inviare</button>
            <button class="sec" onclick={() => (conferma = false)}>Annulla</button>
          </div>
        {/if}
      </div>
    {/if}

    {#if riscrivendo}
      <div class="proposta caricamento"><div class="barra-car"></div><p>Claude sta {modoRiscrittura === "approfondisci" ? "approfondendo" : "rifinendo"} la richiesta, di solito in 5–15 secondi.</p></div>
    {:else if riscrittura}
      <div class="proposta">
        <h3>{modoRiscrittura === "approfondisci" ? "Versione più profonda" : "Versione rifinita"}</h3>
        {#if modoRiscrittura === "approfondisci"}
          <dl class="lenti">
            {#if riscrittura.professionista}<div><dt>Cosa farebbe un professionista</dt><dd>{riscrittura.professionista}</dd></div>{/if}
            {#if riscrittura.materiale}<div><dt>Materiale grezzo</dt><dd>{riscrittura.materiale}</dd></div>{/if}
            {#if riscrittura.metodi}<div><dt>Metodi e strumenti</dt><dd>{riscrittura.metodi}</dd></div>{/if}
            {#if riscrittura.prova}<div><dt>Cosa lo dimostrerebbe</dt><dd>{riscrittura.prova}</dd></div>{/if}
            {#if riscrittura.fuori}<div><dt>Cosa resta fuori</dt><dd>{riscrittura.fuori}</dd></div>{/if}
          </dl>
        {/if}
        <pre class="nuovo">{riscrittura.prompt}</pre>
        {#if riscrittura.domande?.length}
          <h4>Da completare tu</h4>
          <ul>{#each riscrittura.domande as d}<li>{d}</li>{/each}</ul>
        {/if}
        {#if riscrittura.modifiche?.length}
          <h4>Cosa è cambiato</h4>
          <ul class="tenue">{#each riscrittura.modifiche as m}<li>{m}</li>{/each}</ul>
        {/if}
        <div class="azioni"><button class="pri" onclick={usa}>Usa questa versione</button><button class="sec" onclick={() => (riscrittura = null)}>Scarta</button></div>
      </div>
    {/if}
  </section>

  <aside class="controllo" aria-live="polite">
    <div class="testa">
      <h3>Controllo</h3>
      <label class="interr"><input type="checkbox" bind:checked={jev} onchange={programma} /> Jev</label>
    </div>
    {#if !jev}
      <p class="vuoto">Controllo spento. Rifinisci e Approfondisci funzionano lo stesso.</p>
    {:else if parole <= 3}
      <p class="vuoto">Scrivi la richiesta: il controllo parte dalla quarta parola.</p>
    {:else if giudicando && !giudizio}
      <p class="vuoto">Controllo in corso…</p>
    {:else if giudizio?.saltato}
      <p class="vuoto">Non inviato a Jev: il testo o l'ultimo messaggio dell'agente parla di password o credenziali.</p>
    {:else if giudizio?.errore}
      <p class="vuoto err">Controllo non riuscito: {giudizio.errore}</p>
    {:else if giudizio}
      <p class="tipo">{TIPI[giudizio.tipo ?? ""] ?? giudizio.tipo}{giudicando ? " · aggiorno…" : ""}</p>
      {#if !giudizio.compito}
        <p class="vuoto">Per conferme, domande e risposte i punti non servono.</p>
      {:else}
        {#if avvisi.length === 0}<p class="ok-tutto">Nessun avviso: la richiesta dice come verificare e cosa evitare.</p>{/if}
        <ul class="punti">
          {#each PUNTI as p}
            {@const e = giudizio.punti?.[p.k]?.esito}
            <li class:manca={e === "no" && p.avviso} class:info={!p.avviso}>
              <span class="segno">{e === "si" ? "✓" : e === "no" ? (p.avviso ? "!" : "–") : "?"}</span>
              <span class="nome">{p.nome}{p.avviso ? "" : " · informazione"}</span>
              {#if e !== "si"}
                <div class="aggiungi">
                  <label for={"r-" + p.k}>{p.domanda}</label>
                  <div class="campo">
                    <input id={"r-" + p.k} bind:value={risposte[p.k]} placeholder={"es. " + p.esempio} onkeydown={(ev) => ev.key === "Enter" && aggiungi(p)} />
                    <button class="sec" onclick={() => aggiungi(p)} disabled={!risposte[p.k]?.trim()}>Aggiungi</button>
                  </div>
                </div>
              {/if}
            </li>
          {/each}
        </ul>
      {/if}
    {/if}
    <p class="piede">Jev vede solo la richiesta e la fine dell'ultimo messaggio dell'agente, senza percorsi, email e numeri. Non parte nulla che parli di password.</p>
  </aside>

  {#if esito}<div class="esito" class:errore={esito.errore} role="status">{esito.testo}</div>{/if}
</div>

<style>
  .carteggio { display: grid; grid-template-columns: minmax(0, 1fr) 300px; gap: 0; min-height: calc(100vh - 49px); }
  .scrittura { padding: 18px 20px 32px; display: flex; flex-direction: column; gap: 12px; min-width: 0; }
  .riga-dest { display: flex; gap: 8px; align-items: center; flex-wrap: wrap; }
  .riga-dest label { color: var(--testo-3); font-size: 12px; }
  select, input, textarea {
    font: inherit; color: var(--testo); background: var(--pannello); border: 1px solid var(--bordo); border-radius: 6px;
  }
  select { padding: 5px 8px; max-width: 100%; }
  select.cartella { max-width: 320px; }
  textarea { padding: 12px 14px; line-height: 1.55; resize: vertical; min-height: 200px; font-size: 14px; }
  textarea:focus, input:focus, select:focus { outline: 2px solid var(--c-lavora); outline-offset: -1px; }
  .azioni { display: flex; gap: 8px; align-items: center; flex-wrap: wrap; }
  .spazio { flex: 1; }
  .nota { color: var(--c-aspetta); font-size: 12px; }
  button.pri, button.sec { border-radius: 6px; padding: 6px 12px; cursor: pointer; transition: background 150ms, color 150ms, opacity 150ms; }
  button.pri { background: var(--testo); color: var(--bg); border: 1px solid var(--testo); font-weight: 600; }
  button.pri:hover:not(:disabled) { opacity: 0.88; }
  button.sec { background: transparent; color: var(--testo-2); border: 1px solid var(--bordo); }
  button.sec:hover:not(:disabled) { background: var(--riga-hover); color: var(--testo); }
  button:disabled { opacity: 0.4; cursor: default; }

  .conferma { border: 1px solid var(--bordo); background: var(--pannello); border-radius: 8px; padding: 12px 14px; animation: apri 200ms cubic-bezier(0.22, 1, 0.36, 1); }
  .conferma p { margin: 0 0 10px; }
  .conferma div { display: flex; gap: 8px; }
  .mono { font-family: var(--mono); font-size: 12px; }

  .proposta { border-top: 1px solid var(--bordo); padding-top: 14px; animation: apri 200ms cubic-bezier(0.22, 1, 0.36, 1); }
  .proposta h3 { margin: 0 0 8px; font-size: 13px; }
  .proposta h4 { margin: 12px 0 4px; font-size: 11px; text-transform: uppercase; letter-spacing: 0.05em; color: var(--testo-3); }
  .proposta ul { margin: 0; padding-left: 18px; color: var(--testo-2); }
  .proposta ul.tenue { color: var(--testo-3); font-size: 12.5px; }
  .nuovo { white-space: pre-wrap; font: 13.5px/1.55 inherit; font-family: inherit; background: var(--pannello); border: 1px solid var(--bordo); border-radius: 8px; padding: 12px 14px; margin: 0; }
  .lenti { margin: 0 0 10px; display: grid; gap: 8px; }
  .lenti dt { font-size: 11px; text-transform: uppercase; letter-spacing: 0.05em; color: var(--testo-3); }
  .lenti dd { margin: 2px 0 0; color: var(--testo-2); }
  .caricamento p { color: var(--testo-3); margin: 8px 0 0; }
  .barra-car { height: 2px; background: linear-gradient(90deg, transparent, var(--c-lavora), transparent); background-size: 50% 100%; background-repeat: no-repeat; animation: scorri 1.2s linear infinite; }
  @keyframes scorri { from { background-position: -50% 0; } to { background-position: 150% 0; } }

  .controllo { border-left: 1px solid var(--bordo); background: var(--pannello); padding: 18px 16px; display: flex; flex-direction: column; gap: 10px; }
  .testa { display: flex; align-items: center; justify-content: space-between; }
  .testa h3 { margin: 0; font-size: 11px; text-transform: uppercase; letter-spacing: 0.06em; color: var(--testo-3); }
  .interr { color: var(--testo-3); font-size: 12px; display: flex; gap: 6px; align-items: center; }
  .vuoto { color: var(--testo-3); margin: 0; }
  .vuoto.err { color: var(--c-aspetta); }
  .tipo { margin: 0; font-weight: 600; }
  .ok-tutto { margin: 0; color: var(--c-finito); font-size: 12.5px; }
  .punti { list-style: none; padding: 0; margin: 0; display: grid; gap: 10px; }
  .punti li { display: grid; grid-template-columns: 18px 1fr; gap: 2px 8px; align-items: baseline; }
  .segno { font-weight: 700; color: var(--c-finito); text-align: center; }
  .punti li.manca .segno { color: var(--c-aspetta); }
  .punti li.info .segno { color: var(--testo-3); }
  .punti li.manca .nome { color: var(--c-aspetta); font-weight: 600; }
  .punti li.info .nome { color: var(--testo-2); }
  .aggiungi { grid-column: 2; display: grid; gap: 4px; margin-top: 2px; }
  .aggiungi label { font-size: 12px; color: var(--testo-3); }
  .campo { display: flex; gap: 6px; }
  .campo input { flex: 1; min-width: 0; padding: 4px 8px; font-size: 12.5px; }
  .campo button { padding: 4px 8px; font-size: 12px; }
  .piede { margin-top: auto; font-size: 11.5px; color: var(--testo-3); }

  .esito {
    position: fixed; left: 50%; bottom: 18px; transform: translateX(-50%); max-width: min(560px, calc(100vw - 32px));
    padding: 8px 14px; border-radius: 8px; background: var(--pannello); border: 1px solid var(--bordo); z-index: 5;
    box-shadow: 0 6px 24px oklch(0 0 0 / 0.25); animation: apri 200ms cubic-bezier(0.22, 1, 0.36, 1);
  }
  .esito.errore { border-color: var(--c-aspetta); }
  @keyframes apri { from { opacity: 0; transform: translateY(-4px); } }
  @media (max-width: 760px) { .carteggio { grid-template-columns: 1fr; } .controllo { border-left: 0; border-top: 1px solid var(--bordo); } }
</style>
