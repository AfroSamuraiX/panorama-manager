<script lang="ts">
  import Sidebar from "$lib/components/Sidebar.svelte";

  const { active, title, summary } = $props<{
    active: "media" | "cooling" | "settings";
    title: string;
    summary: string;
  }>();
</script>

<svelte:head>
  <title>panorama-mgr · {title}</title>
</svelte:head>

<main class="shell">
  <Sidebar {active} />

  <section class="content">
    <header class="hero">
      <div>
        <p class="eyebrow">Coming soon</p>
        <h1>{title}</h1>
        <p class="summary">{summary}</p>
      </div>
    </header>

    <section class="panel">
      <p class="status-label">Work in progress</p>
      <h2>This page is intentionally stubbed.</h2>
      <p>
        The navigation target is wired so layout and packaging can settle before this feature is
        implemented.
      </p>
    </section>
  </section>
</main>

<style>
  :global(*) {
    box-sizing: border-box;
  }

  :global(:root) {
    --bg: #070808;
    --surface-soft: rgba(242, 240, 234, 0.055);
    --line: rgba(242, 240, 234, 0.12);
    --text: #f2f0ea;
    --muted: #9da39c;
    --accent: #9ef0b8;
    --accent-strong: #c9ffd8;
    --accent-deep: #2f7d58;
    --warning-color: #f0b84a;
    --danger: #ff6f7d;
  }

  :global(body) {
    margin: 0;
    min-width: 960px;
    color: var(--text);
    background:
      radial-gradient(circle at 18% 12%, rgba(158, 240, 184, 0.1), transparent 30%),
      radial-gradient(circle at 82% 0%, rgba(47, 125, 88, 0.16), transparent 34%),
      var(--bg);
    font-family:
      "Adwaita Sans", "Cantarell", "Noto Sans", ui-sans-serif, system-ui, sans-serif;
    font-feature-settings: "ss01", "tnum";
  }

  .shell {
    display: grid;
    grid-template-columns: 112px 1fr;
    min-height: 100vh;
  }

  .content {
    padding: 48px;
  }

  .hero {
    padding-bottom: 34px;
    border-bottom: 1px solid var(--line);
  }

  .eyebrow,
  .status-label {
    margin: 0 0 10px;
    color: var(--accent-strong);
    font-size: 13px;
    font-weight: 800;
    letter-spacing: 0.16em;
    text-transform: uppercase;
  }

  h1,
  h2,
  p {
    margin-top: 0;
  }

  h1 {
    margin-bottom: 12px;
    font-size: clamp(40px, 6vw, 72px);
    letter-spacing: -0.07em;
  }

  .summary,
  .panel p:not(.status-label) {
    max-width: 720px;
    color: var(--muted);
    font-size: 18px;
    line-height: 1.6;
  }

  .panel {
    margin-top: 34px;
    padding: 28px;
    border: 1px solid var(--line);
    border-radius: 28px;
    background: linear-gradient(145deg, rgba(24, 27, 26, 0.82), rgba(11, 13, 12, 0.86));
    box-shadow: 0 24px 80px rgba(0, 0, 0, 0.32);
  }
</style>
