<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { onMount } from "svelte";

  type Sessione = {
    id: string; agente: "claude" | "codex" | "pi"; progetto: string; cartella: string | null; titolo: string | null;
    stato: string; azione: string; posizione: { app: string } | null;
    richiesta: string | null; ultima_risposta: string | null;
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
  let cmuxStato = $state<{ installato: boolean; accesso: boolean; password_salvata: boolean } | null>(null);
  let primoConferma = $state<HTMLButtonElement | null>(null);
  // la conferma compare sotto il campo: la si porta in vista col fuoco sul pulsante principale (Invio conferma)
  $effect(() => {
    if (primoConferma) {
      primoConferma.scrollIntoView({ block: "nearest", behavior: "smooth" });
      primoConferma.focus();
    }
  });
  async function apriConferma() {
    conferma = true;
    cmuxStato = null;
    if (scelta?.posizione?.app === "cmux" && !anteprima) {
      cmuxStato = await invoke<{ installato: boolean; accesso: boolean; password_salvata: boolean }>("stato_cmux").catch(() => null);
    }
  }
  let esito = $state<{ testo: string; errore: boolean } | null>(null);
  let agenteNuovo = $state<"claude" | "codex" | "pi">("claude");
  let cartellaNuova = $state("");
  let cartelleScelte = $state<string[]>([]);
  // Cervello centrale facoltativo: Crew Deck mette solo una riga in testa al primo messaggio, non scrive file.
  let cervello = $state("");
  let rigaCervello = $state("");
  let conCervello = $state(true);
  let spiegaCervello = $state(false);
  let creaIn = $state<string | null>(null);
  let timer: ReturnType<typeof setTimeout> | undefined;
  let tuttaRisposta = $state(false);
  let suggerimenti = $state<{ testo: string; perche: string }[] | null>(null);
  let suggerendo = $state(false);
  let ultimoGiudicato = "";

  const aperte = $derived(sessioni.filter((s) => s.posizione && !s.id.startsWith("_")));
  const cartelle = $derived([...new Set([...cartelleScelte, ...sessioni.map((s) => s.cartella).filter((c): c is string => !!c)])].slice(0, 30));
  const breve = (p: string) => p.replace(/^\/Users\/[^/]+/, "~");
  const rigaAttiva = $derived(destinazione === "nuova" && !!cervello && conCervello);
  async function scegli(titolo: string): Promise<string | null> {
    try { return await invoke<string | null>("scegli_cartella", { titolo }); }
    catch (e) { mostra(String(e), true); return null; }
  }
  async function scegliCartella() {
    const c = await scegli("Cartella di lavoro");
    if (c) { cartelleScelte = [c, ...cartelleScelte.filter((x) => x !== c)]; cartellaNuova = c; }
  }
  async function collegaCervello() {
    const c = await scegli("Cartella del cervello centrale");
    if (c) cervello = c;
  }
  async function scegliDoveCreare() {
    creaIn = await scegli("Dove creare il cervello centrale");
  }
  // la cartella scelta non basta: prima di aprire l'agente si chiede conferma, come per ogni invio
  async function creaCervello() {
    const c = creaIn;
    creaIn = null;
    if (!c) return;
    spiegaCervello = false;
    try { mostra(await invoke<string>("crea_cervello", { agente: agenteNuovo, cartella: c })); }
    catch (e) { mostra(String(e), true); }
  }
  $effect(() => {
    const c = cervello;
    if (!c) { rigaCervello = ""; return; }
    if (anteprima) { rigaCervello = `Cervello centrale: ${c}. Prima di cominciare leggi le sue istruzioni. …\n\n`; return; }
    rigaCervello = "";
    invoke<string>("riga_cervello", { percorso: c }).then((r) => { if (cervello === c) rigaCervello = r; }).catch(() => {});
  });
  const scelta = $derived(destinazione && destinazione !== "nuova" ? sessioni.find((s) => `${s.agente}:${s.id}` === destinazione) ?? null : null);
  // Domande che l'agente ha fatto nell'ultima risposta: le frasi che finiscono con «?».
  const domandeAgente = $derived.by(() => {
    const t = (scelta?.ultima_risposta ?? "").slice(-2500);
    const pezzi = t.split(/\n+|(?<=[.!?])\s+(?=[A-ZÀ-Ý«*])/);
    const viste = new Set<string>();
    return pezzi
      .map((x) => x.replace(/\*\*/g, "").replace(/^\s*([-•]|\d+[.)])\s*/, "").trim())
      .filter((x) => x.endsWith("?") && x.length >= 8 && x.length <= 240 && !viste.has(x) && viste.add(x))
      .slice(-4);
  });
  // Testo leggibile: via i segni della formattazione Markdown più comuni.
  const leggibile = (t: string) => t.replace(/\*\*(.+?)\*\*/g, "$1").replace(/`([^`]+)`/g, "$1").replace(/^#{1,6}\s+/gm, "");
  const STATI: Record<string, string> = { aspetta: "Ti aspetta", lavora: "Sta lavorando", finito: "Ha finito", inattivo: "In porto" };
  function rispondiA(d: string) {
    const breve = d.length > 90 ? d.slice(0, 87) + "…" : d;
    testo = (testo.trim() ? testo.trimEnd() + "\n" : "") + `Su «${breve}»: `;
    programma();
  }
  async function suggerisci() {
    if (!scelta) return;
    suggerendo = true; suggerimenti = null;
    try {
      const r = await invoke<{ suggerimenti: { testo: string; perche: string }[] }>("suggerisci_risposta", { agente: scelta.agente, id: scelta.id });
      suggerimenti = r.suggerimenti ?? [];
    } catch (e) {
      mostra(String(e), true);
    } finally {
      suggerendo = false;
    }
  }
  function usaSuggerimento(t: string) { testo = t; suggerimenti = null; programma(); }
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
    // si fotografa ciò che è stato confermato prima di ogni attesa: il clic autorizza esattamente questo invio
    const nuova = destinazione === "nuova";
    const s = scelta;
    const ag = agenteNuovo, cart = cartellaNuova, t = testo, cerv = rigaAttiva ? cervello : "";
    try {
      // la riga si calcola al momento dell'invio dal cervello confermato, non dall'anteprima
      const riga = cerv && !anteprima ? await invoke<string>("riga_cervello", { percorso: cerv }) : "";
      const msg = nuova
        ? await invoke<string>("nuova_sessione", { agente: ag, cartella: cart, testo: riga + t })
        : await invoke<string>("invia_prompt", { agente: s!.agente, id: s!.id, testo: t, invio: conInvio });
      mostra(msg);
    } catch (e) {
      mostra(String(e), true);
    }
  }

  onMount(() => {
    jev = localStorage.getItem("crewdeck.jev") !== "0";
    if (!destinazione) destinazione = aperte[0] ? `${aperte[0].agente}:${aperte[0].id}` : "nuova";
    if (!cartellaNuova && cartelle[0]) cartellaNuova = cartelle[0];
    cervello = localStorage.getItem("crewdeck.cervello") ?? "";
  });
  $effect(() => localStorage.setItem("crewdeck.jev", jev ? "1" : "0"));
  $effect(() => { if (cervello) localStorage.setItem("crewdeck.cervello", cervello); else localStorage.removeItem("crewdeck.cervello"); });
</script>

<div class="carteggio">
  <section class="scrittura">
    <div class="riga-dest">
      <label for="dest">A chi</label>
      <select id="dest" bind:value={destinazione} onchange={() => { ultimoGiudicato = ""; suggerimenti = null; tuttaRisposta = false; programma(); }}>
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
          {#each cartelle as c}<option value={c}>{breve(c)}</option>{/each}
        </select>
        <button class="sec" onclick={scegliCartella}>Scegli…</button>
      {/if}
    </div>
    {#if destinazione === "nuova"}
      <div class="cervello">
        <span class="etichetta">Cervello centrale <button class="interr-btn" aria-label="Cos'è il cervello centrale?" aria-expanded={spiegaCervello} onclick={() => (spiegaCervello = !spiegaCervello)}>?</button></span>
        {#if cervello}
          <span class="mono">{breve(cervello)}</span>
          <button class="link" onclick={collegaCervello}>Cambia</button> · <button class="link" onclick={() => (cervello = "")}>Togli</button>
        {:else}
          <button class="link" onclick={collegaCervello}>Collega…</button>
        {/if}
      </div>
      {#if spiegaCervello}
        <div class="spiega">
          <p>Un cervello centrale è una cartella di note che gli agenti leggono per conoscere i tuoi progetti, ricordare le decisioni prese e riprendere il lavoro senza perdere il contesto.</p>
          <p>Se lo colleghi, Crew Deck mette una sola riga in testa al primo messaggio di ogni sessione nuova: dice all'agente dove leggere le istruzioni, come creare il cervello del progetto se manca e dove annotare il lavoro fatto. Crew Deck non scrive file: lo fa l'agente, seguendo le regole del tuo cervello.</p>
          <p>Non ce l'hai? Puoi farne a meno, oppure fartene creare uno: scegli una cartella e un agente ti guida passo passo.</p>
          {#if creaIn}
            <p>Apro una sessione di <strong>{agenteNuovo}</strong> in <span class="mono">{breve(creaIn)}</span> che crea lì un cervello centrale, guidandoti passo passo?</p>
            <div class="azioni"><button class="pri" onclick={creaCervello}>Apri</button><button class="sec" onclick={() => (creaIn = null)}>Annulla</button></div>
          {:else}
            <div class="azioni"><button class="sec" onclick={scegliDoveCreare}>Creane uno</button><button class="link" onclick={() => (spiegaCervello = false)}>Chiudi</button></div>
          {/if}
        </div>
      {/if}
      {#if cervello}
        <label class="riga-cervello"><input type="checkbox" bind:checked={conCervello} /> In testa al messaggio: <span class="tenue">{rigaCervello ? `«${rigaCervello.trim()}»` : "la riga del cervello centrale…"}</span></label>
      {/if}
    {/if}

    {#if scelta}
      <div class="conversazione">
        <div class="conv-testa">
          <h3>Dove eravate rimasti</h3>
          <span class="conv-stato {scelta.stato}">{STATI[scelta.stato] ?? scelta.stato}{scelta.stato === "lavora" || scelta.stato === "aspetta" ? ` · ${scelta.azione}` : ""}</span>
        </div>
        {#if scelta.richiesta}
          <div class="battuta tu"><span class="chi">Tu</span><p class="taglia3">{leggibile(scelta.richiesta)}</p></div>
        {/if}
        {#if scelta.ultima_risposta}
          <div class="battuta agente">
            <span class="chi">{scelta.agente === "claude" ? "Claude" : scelta.agente === "codex" ? "Codex" : "pi"}</span>
            <p class:taglia={!tuttaRisposta}>{leggibile(scelta.ultima_risposta)}</p>
            {#if scelta.ultima_risposta.length > 500}
              <button class="link" onclick={() => (tuttaRisposta = !tuttaRisposta)}>{tuttaRisposta ? "Mostra meno" : "Mostra tutta la risposta"}</button>
            {/if}
          </div>
        {:else}
          <p class="vuoto">L'agente non ha ancora risposto.</p>
        {/if}
        {#if domandeAgente.length}
          <div class="domande-ag">
            <span class="chi">Ti ha chiesto</span>
            <ul>{#each domandeAgente as d}<li><button class="dom" onclick={() => rispondiA(d)} title="Prepara una risposta a questa domanda">{d}</button></li>{/each}</ul>
          </div>
        {/if}
        <div class="sugg-riga">
          <button class="sec" onclick={suggerisci} disabled={suggerendo || !scelta.ultima_risposta}>{suggerendo ? "Penso a cosa rispondere…" : "Cosa rispondere?"}</button>
          <span class="nota-t">Claude propone 2–3 risposte possibili, tu scegli e modifichi.</span>
        </div>
        {#if suggerimenti}
          <ul class="suggerimenti">
            {#each suggerimenti as sg}
              <li><button onclick={() => usaSuggerimento(sg.testo)}><span class="s-testo">{sg.testo}</span><span class="s-perche">{sg.perche}</span></button></li>
            {/each}
          </ul>
        {/if}
      </div>
    {/if}

    <textarea bind:value={testo} oninput={programma} placeholder="Scrivi la richiesta. Il controllo parte mentre scrivi." rows="7" spellcheck="true"></textarea>

    <div class="azioni">
      <button class="sec" disabled={!testo.trim() || riscrivendo} onclick={() => riscrivi("rifinisci")} title="Modifiche minime: aggiunge solo ciò che manca">
        {riscrivendo && modoRiscrittura === "rifinisci" ? "Rifinisco…" : "Rifinisci"}
      </button>
      <button class="sec" disabled={!testo.trim() || riscrivendo} onclick={() => riscrivi("approfondisci")} title="Cosa farebbe un professionista, da quale materiale, con quali metodi, cosa lo dimostrerebbe, cosa resta fuori">
        {riscrivendo && modoRiscrittura === "approfondisci" ? "Approfondisco…" : "Approfondisci"}
      </button>
      <span class="spazio"></span>
      {#if segnaposti}<span class="nota">{segnaposti} {segnaposti === 1 ? "segnaposto da compilare" : "segnaposti da compilare"}</span>{/if}
      <button class="pri" disabled={!testo.trim() || (destinazione !== "nuova" && !scelta) || (destinazione === "nuova" && !cartellaNuova)} onclick={apriConferma}>
        {destinazione === "nuova" ? "Apri sessione…" : "Invia…"}
      </button>
    </div>

    {#if conferma}
      <div class="conferma" role="alertdialog" aria-label="Conferma invio">
        {#if destinazione === "nuova"}
          <p>Apro una sessione di <strong>{agenteNuovo}</strong> in <span class="mono">{cartellaNuova.replace(/^\/Users\/[^/]+/, "~")}</span> con questa richiesta{rigaAttiva ? ", con la riga del cervello centrale" : ""}?</p>
          <div><button class="pri" bind:this={primoConferma} onclick={() => invia(true)}>Apri</button><button class="sec" onclick={() => (conferma = false)}>Annulla</button></div>
        {:else}
          {#if scelta?.posizione?.app === "cmux" && cmuxStato && !cmuxStato.accesso}
            <div class="avviso-cmux">
              <strong>cmux non accetta ancora comandi da Crew Deck.</strong> Se continui, il prompt va negli appunti e lo incolli tu con ⌘V.
              Per l'invio diretto, una volta sola:
              <ol>
                <li>in cmux: <em>Impostazioni › Automazione</em>, modalità <em>password</em>, scegli una password;</li>
                <li>{cmuxStato.password_salvata ? "la password è già nel portachiavi: controlla che sia la stessa" : "salvala nel portachiavi da Terminale:"}{#if !cmuxStato.password_salvata} <code>security add-generic-password -s crew-deck -a cmux -w</code>{/if}</li>
              </ol>
            </div>
          {/if}
          <p>Scrivo la richiesta nella scheda di <strong>{scelta?.progetto}</strong> ({scelta?.posizione?.app}){scelta?.stato === "lavora" ? ": l'agente sta lavorando, la riceverà in coda" : ""}.{#if segnaposti} Restano {segnaposti} segnaposti [[…]].{/if}</p>
          <div>
            <button class="pri" bind:this={primoConferma} onclick={() => invia(true)}>Scrivi e invia</button>
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
  .cervello { display: flex; gap: 6px; align-items: center; flex-wrap: wrap; font-size: 12px; color: var(--testo-2); }
  .cervello .etichetta { color: var(--testo-3); }
  .interr-btn { width: 16px; height: 16px; border-radius: 50%; border: 1px solid var(--bordo); background: transparent; color: var(--testo-3); font-size: 10px; line-height: 1; padding: 0; cursor: pointer; }
  .spiega { border: 1px solid var(--bordo); background: var(--pannello); border-radius: 8px; padding: 10px 14px; font-size: 13px; }
  .spiega p { margin: 0 0 8px; }
  .spiega .azioni { display: flex; gap: 12px; align-items: center; }
  .riga-cervello { font-size: 12px; color: var(--testo-2); display: flex; gap: 6px; align-items: baseline; }
  .riga-cervello .tenue { color: var(--testo-3); }
  select, input, textarea {
    font: inherit; color: var(--testo); background: var(--pannello); border: 1px solid var(--bordo); border-radius: 6px;
  }
  select { padding: 5px 8px; max-width: 100%; }
  select.cartella { max-width: 320px; }
  textarea { padding: 12px 14px; line-height: 1.55; resize: vertical; min-height: 140px; font-size: 14px; }
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
  .avviso-cmux { border: 1px solid var(--c-aspetta); background: var(--tinta-aspetta); border-radius: 6px; padding: 10px 12px; margin-bottom: 10px; color: var(--testo-2); }
  .avviso-cmux strong { color: var(--testo); }
  .avviso-cmux ol { margin: 6px 0 0; padding-left: 18px; display: grid; gap: 4px; }
  .avviso-cmux code { font-family: var(--mono); font-size: 11.5px; background: var(--bg); padding: 1px 5px; border-radius: 4px; user-select: all; }

  .conversazione { border: 1px solid var(--bordo); border-radius: 8px; padding: 12px 14px; display: grid; gap: 10px; background: var(--pannello); }
  .conv-testa { display: flex; align-items: baseline; justify-content: space-between; gap: 12px; }
  .conv-testa h3 { margin: 0; font-size: 11px; text-transform: uppercase; letter-spacing: 0.06em; color: var(--testo-3); }
  .conv-stato { font-size: 12px; color: var(--testo-3); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; max-width: 60%; }
  .conv-stato.aspetta { color: var(--c-aspetta); }
  .conv-stato.lavora { color: var(--c-lavora); }
  .battuta { display: grid; grid-template-columns: 56px minmax(0, 1fr); gap: 2px 10px; }
  .battuta p { margin: 0; white-space: pre-wrap; overflow-wrap: anywhere; line-height: 1.5; }
  .battuta.tu p { color: var(--testo); }
  .battuta.agente p { color: var(--testo-2); }
  .battuta .link { grid-column: 2; justify-self: start; }
  .chi { font-size: 11px; text-transform: uppercase; letter-spacing: 0.05em; color: var(--testo-3); padding-top: 2px; }
  .taglia3 { display: -webkit-box; -webkit-line-clamp: 3; line-clamp: 3; -webkit-box-orient: vertical; overflow: hidden; }
  .taglia { display: -webkit-box; -webkit-line-clamp: 8; line-clamp: 8; -webkit-box-orient: vertical; overflow: hidden; }
  .link { border: 0; background: transparent; color: var(--c-lavora); padding: 0; cursor: pointer; font: inherit; font-size: 12px; }
  .link:hover { text-decoration: underline; }
  .domande-ag { display: grid; grid-template-columns: 56px minmax(0, 1fr); gap: 2px 10px; }
  .domande-ag ul { list-style: none; margin: 0; padding: 0; display: grid; gap: 4px; }
  .dom { text-align: left; border: 1px solid var(--bordo); background: var(--bg); color: var(--testo); border-radius: 6px; padding: 5px 9px; cursor: pointer; font: inherit; font-size: 12.5px; transition: border-color 150ms; }
  .dom:hover { border-color: var(--c-lavora); }
  .sugg-riga { display: flex; gap: 10px; align-items: center; flex-wrap: wrap; }
  .nota-t { color: var(--testo-3); font-size: 12px; }
  .suggerimenti { list-style: none; margin: 0; padding: 0; display: grid; gap: 6px; }
  .suggerimenti button { width: 100%; text-align: left; display: grid; gap: 3px; border: 1px solid var(--bordo); background: var(--bg); border-radius: 8px; padding: 9px 12px; cursor: pointer; font: inherit; color: var(--testo); transition: border-color 150ms; }
  .suggerimenti button:hover { border-color: var(--c-lavora); }
  .s-testo { white-space: pre-wrap; }
  .s-perche { color: var(--testo-3); font-size: 12px; }
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
