<script lang="ts">
  // Icone di stato disegnate a mano, stesso tratto per tutte.
  let { stato, size = 18 }: { stato: "aspetta" | "lavora" | "finito" | "inattivo"; size?: number } = $props();
</script>

<svg class="icona {stato}" width={size} height={size} viewBox="0 0 20 20" aria-hidden="true">
  {#if stato === "aspetta"}
    <!-- bandiera di segnalazione: qualcuno chiama lo skipper -->
    <circle class="alone" cx="10" cy="10" r="9" />
    <path d="M6.5 16.5V3.5" class="tratto" />
    <path d="M6.5 4h7.5l-2.2 3 2.2 3H6.5z" class="pieno" />
  {:else if stato === "lavora"}
    <!-- vela, con un arco che gira finché lavora -->
    <path d="M9.5 4v9.5H5z" class="pieno" />
    <path d="M10.8 6v7.5h3.7z" class="pieno tenue" />
    <path d="M4.5 15.5h11" class="tratto" />
    <circle class="giro" cx="10" cy="10" r="9" />
  {:else if stato === "finito"}
    <circle cx="10" cy="10" r="8" class="anello" />
    <path d="M6.6 10.3l2.3 2.3 4.6-5" class="tratto" />
  {:else}
    <!-- ancora: in porto -->
    <circle cx="10" cy="4.6" r="1.6" class="tratto" />
    <path d="M10 6.2v10M6.8 9h6.4M4.5 12.5c.8 2.4 3 4 5.5 4s4.7-1.6 5.5-4" class="tratto" />
  {/if}
</svg>

<style>
  .icona { flex: none; overflow: visible; }
  .tratto { fill: none; stroke: currentColor; stroke-width: 1.6; stroke-linecap: round; stroke-linejoin: round; }
  .pieno { fill: currentColor; }
  .tenue { opacity: 0.55; }
  .anello { fill: none; stroke: currentColor; stroke-width: 1.6; opacity: 0.9; }
  .aspetta { color: var(--c-aspetta); }
  .lavora { color: var(--c-lavora); }
  .finito { color: var(--c-finito); }
  .inattivo { color: var(--c-inattivo); }
  .alone { fill: var(--c-aspetta); opacity: 0.16; transform-origin: 10px 10px; animation: alone 1.6s cubic-bezier(0.22, 1, 0.36, 1) infinite; }
  .giro { fill: none; stroke: currentColor; stroke-width: 1.4; stroke-dasharray: 14 43; stroke-linecap: round; opacity: 0.8; transform-origin: 10px 10px; animation: giro 1.4s linear infinite; }
  @keyframes alone { from { transform: scale(0.6); opacity: 0.35; } to { transform: scale(1.15); opacity: 0; } }
  @keyframes giro { to { transform: rotate(360deg); } }
  @media (prefers-reduced-motion: reduce) { .alone, .giro { animation: none; } }
</style>
