<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { onMount } from "svelte";

  type Dati = {
    giorni: number; totale: number; giudicati: number; compiti: number; parole_mediana: number | null;
    saltati: { brevi: number; sistema: number; sensibili: number; errori: number };
    tipi: Record<string, number>;
    quote: { criterio_fine: number | null; vincoli: number | null; perche: number | null; contesto: number | null };
    per_progetto: { progetto: string; compiti: number; con_verifica: number }[];
  };
  const TIPI: Record<string, string> = {
    new_task: "Compiti nuovi", question: "Domande", approval_or_choice: "Conferme e scelte", answer_to_agent: "Risposte all'agente", comment: "Commenti",
  };

  let giorni = $state(7);
  let dati = $state<Dati | null>(null);
  let errore = $state<string | null>(null);

  async function carica() {
    errore = null;
    try {
      dati = "__TAURI_INTERNALS__" in window
        ? await invoke<Dati>("debriefing", { giorni })
        : await fetch("/debriefing.json").then((r) => r.json());
    } catch (e) {
      errore = String(e);
    }
  }
  onMount(carica);

  const consigli = $derived.by(() => {
    if (!dati || dati.compiti < 3) return [];
    const c: { titolo: string; testo: string }[] = [];
    const q = dati.quote;
    if (q.criterio_fine != null && q.criterio_fine < 40)
      c.push({ titolo: "Di' come si verifica", testo: `Solo il ${q.criterio_fine}% dei tuoi compiti dice come capire che è fatto. Aggiungi una riga «È fatto quando…»: un test che passa, un risultato da vedere, un esempio. È il consiglio su cui Anthropic insiste di più.` });
    if (q.vincoli != null && q.vincoli < 30)
      c.push({ titolo: "Metti un confine", testo: `Il ${q.vincoli}% dei compiti pone dei limiti. Una riga su cosa non cambiare o cosa resta fuori evita lavoro in più da disfare.` });
    if (q.perche != null && q.perche < 25)
      c.push({ titolo: "Una frase sul perché", testo: `Il ${q.perche}% spiega a cosa serve il risultato. Quando ci sono due strade possibili, il perché aiuta l'agente a scegliere quella giusta.` });
    return c.slice(0, 3);
  });
</script>

<div class="debrief">
  <header>
    <h2>Debriefing</h2>
    <div class="periodo" role="tablist" aria-label="Periodo">
      {#each [7, 30, 90] as g}
        <button role="tab" aria-selected={giorni === g} onclick={() => { giorni = g; carica(); }}>{g} giorni</button>
      {/each}
    </div>
  </header>

  {#if errore}
    <p class="vuoto">Non riesco a leggere il registro: {errore}</p>
  {:else if !dati}
    <p class="vuoto">Caricamento…</p>
  {:else if dati.totale === 0}
    <div class="vuoto grande">
      <p class="t">Ancora nessun dato</p>
      <p>Il giudice registra ogni prompt di Claude Code da quando è attivo l'hook. Torna dopo qualche sessione.</p>
    </div>
  {:else}
    <section class="sintesi">
      <div><span class="n">{dati.giudicati}</span><span class="l">prompt giudicati</span></div>
      <div><span class="n">{dati.compiti}</span><span class="l">compiti nuovi</span></div>
      <div><span class="n">{dati.parole_mediana ?? "–"}</span><span class="l">parole per compito, mediana</span></div>
    </section>

    {#if consigli.length}
      <section>
        <h3>Tre cose da provare</h3>
        <ol class="consigli">
          {#each consigli as c}<li><strong>{c.titolo}.</strong> {c.testo}</li>{/each}
        </ol>
      </section>
    {/if}

    <section>
      <h3>Nei compiti nuovi</h3>
      <ul class="barre">
        {#each [["criterio_fine", "dicono come si verifica"], ["vincoli", "pongono vincoli"], ["perche", "spiegano il perché"], ["contesto", "danno contesto che l'agente non vede"]] as [k, nome]}
          {@const v = dati.quote[k as keyof Dati["quote"]]}
          <li>
            <span class="nome">{nome}</span>
            <span class="traccia"><span class="riemp" class:basso={v != null && v < 40} style={`width:${v ?? 0}%`}></span></span>
            <span class="pct">{v == null ? "–" : `${v}%`}</span>
          </li>
        {/each}
      </ul>
    </section>

    <div class="due">
      <section>
        <h3>Che tipo di messaggi scrivi</h3>
        <table>
          <tbody>
            {#each Object.entries(dati.tipi).sort((a, b) => b[1] - a[1]) as [t, n]}
              <tr><td>{TIPI[t] ?? t}</td><td class="num">{n}</td></tr>
            {/each}
            <tr class="tenue"><td>Brevi, non giudicati</td><td class="num">{dati.saltati.brevi}</td></tr>
            <tr class="tenue"><td>Sensibili, non inviati</td><td class="num">{dati.saltati.sensibili}</td></tr>
          </tbody>
        </table>
      </section>
      <section>
        <h3>Per progetto</h3>
        <table>
          <thead><tr><th>Progetto</th><th class="num">Compiti</th><th class="num">Con verifica</th></tr></thead>
          <tbody>
            {#each [...dati.per_progetto].sort((a, b) => b.compiti - a.compiti).slice(0, 10) as p}
              <tr><td>{p.progetto}</td><td class="num">{p.compiti}</td><td class="num">{Math.round((p.con_verifica / p.compiti) * 100)}%</td></tr>
            {/each}
          </tbody>
        </table>
      </section>
    </div>
    <p class="piede">Dati dall'hook del giudice: solo numeri, mai il testo dei prompt. Le soglie vengono dalla taratura (giudice/TARATURA.md).</p>
  {/if}
</div>

<style>
  .debrief { padding: 18px 24px 40px; max-width: 920px; }
  header { display: flex; align-items: center; justify-content: space-between; margin-bottom: 18px; }
  h2 { margin: 0; font-size: 16px; }
  h3 { margin: 0 0 10px; font-size: 11px; text-transform: uppercase; letter-spacing: 0.06em; color: var(--testo-3); font-weight: 600; }
  section { margin-bottom: 26px; }
  .periodo { display: flex; gap: 2px; padding: 3px; border: 1px solid var(--bordo); border-radius: 9px; }
  .periodo button { border: 0; background: transparent; color: var(--testo-2); padding: 4px 10px; border-radius: 6px; cursor: pointer; }
  .periodo button[aria-selected="true"] { background: var(--riga-hover); color: var(--testo); }
  .sintesi { display: flex; gap: 40px; border-bottom: 1px solid var(--bordo); padding-bottom: 18px; }
  .sintesi div { display: flex; flex-direction: column; }
  .sintesi .n { font-size: 26px; font-weight: 600; font-variant-numeric: tabular-nums; }
  .sintesi .l { color: var(--testo-3); font-size: 12px; }
  .consigli { margin: 0; padding-left: 20px; display: grid; gap: 8px; color: var(--testo-2); max-width: 70ch; }
  .consigli strong { color: var(--testo); }
  .barre { list-style: none; padding: 0; margin: 0; display: grid; gap: 10px; max-width: 640px; }
  .barre li { display: grid; grid-template-columns: 240px 1fr 48px; gap: 12px; align-items: center; }
  .barre .nome { color: var(--testo-2); }
  .traccia { height: 6px; background: var(--riga-hover); border-radius: 3px; overflow: hidden; }
  .riemp { display: block; height: 100%; background: var(--c-finito); border-radius: 3px; transition: width 400ms cubic-bezier(0.22, 1, 0.36, 1); }
  .riemp.basso { background: var(--c-aspetta); }
  .pct { text-align: right; font-variant-numeric: tabular-nums; color: var(--testo-2); }
  .due { display: grid; grid-template-columns: 1fr 1fr; gap: 32px; }
  table { width: 100%; border-collapse: collapse; }
  th { text-align: left; font-weight: 500; color: var(--testo-3); font-size: 12px; padding: 0 0 6px; }
  td { padding: 6px 0; border-top: 1px solid var(--bordo); }
  .num { text-align: right; font-variant-numeric: tabular-nums; }
  tr.tenue td { color: var(--testo-3); }
  .vuoto { color: var(--testo-3); }
  .vuoto.grande { text-align: center; margin-top: 14vh; }
  .vuoto .t { color: var(--testo); font-weight: 600; font-size: 14px; margin-bottom: 4px; }
  .piede { color: var(--testo-3); font-size: 11.5px; }
  @media (max-width: 720px) { .due { grid-template-columns: 1fr; } .barre li { grid-template-columns: 1fr 1fr 40px; } }
</style>
