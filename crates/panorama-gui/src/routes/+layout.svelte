<script lang="ts">
  import type { Snippet } from "svelte";
  import icon from "$lib/assets/aio-icon.png";

  const { children } = $props<{ children: Snippet }>();
</script>

<svelte:head>
  <link rel="icon" type="image/png" href={icon} />
</svelte:head>

{@render children()}
