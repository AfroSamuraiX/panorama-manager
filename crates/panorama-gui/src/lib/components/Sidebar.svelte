<script lang="ts">
  import icon from "$lib/assets/aio-icon.png";

  const { active } = $props<{ active: "overview" | "display" | "media" | "cooling" | "settings" }>();

  const items = [
    { id: "overview", label: "Overview", href: "/" },
    { id: "display", label: "Display", href: "/display" },
    { id: "media", label: "Media", href: "/media" },
    { id: "cooling", label: "Cooling", href: "/cooling" },
    { id: "settings", label: "Settings", href: "/settings" },
  ] as const;
</script>

<nav class="sidebar" aria-label="Primary">
  <div class="mark" aria-label="AIO cooler">
    <img src={icon} alt="" aria-hidden="true" />
  </div>
  {#each items as item}
    <a class:active={active === item.id} class="nav-item" href={item.href}>{item.label}</a>
  {/each}
</nav>

<style>
  .sidebar {
    display: flex;
    flex-direction: column;
    gap: 14px;
    padding: 24px 14px;
    border-right: 1px solid var(--line);
    background: rgba(6, 7, 7, 0.76);
    color: var(--muted);
    backdrop-filter: blur(18px);
  }

  .mark {
    display: grid;
    place-items: center;
    width: 56px;
    height: 56px;
    margin: 0 auto 34px;
    border-radius: 18px;
    background: linear-gradient(135deg, var(--accent), var(--accent-deep));
    color: #06100b;
    box-shadow: 0 18px 40px rgba(158, 240, 184, 0.18);
  }

  .mark img {
    width: 40px;
    height: 40px;
    object-fit: contain;
  }
  .nav-item {
    display: block;
    padding: 14px 10px;
    border-radius: 16px;
    color: inherit;
    font-size: 13px;
    font-weight: 700;
    text-align: center;
    text-decoration: none;
  }

  .nav-item.active,
  .nav-item:hover {
    background: rgba(158, 240, 184, 0.1);
    color: var(--text);
  }
</style>
