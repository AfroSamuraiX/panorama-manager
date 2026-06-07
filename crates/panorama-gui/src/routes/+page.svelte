<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import DisplaySurface from "$lib/components/DisplaySurface.svelte";
  import Sidebar from "$lib/components/Sidebar.svelte";
  import { emptyDisplay, type CurrentDisplay } from "$lib/display";
  import { onMount } from "svelte";

  type DeviceInfo = {
    productId: string;
    os: string;
    serial: string;
    appVersion: string;
    firmware: string;
    hardware: string;
    attributes: string[];
  };

  type DeviceWarning = {
    kind: string;
    description: string;
  };

  type OverviewStatus = {
    daemonAvailable: boolean;
    device: DeviceInfo | null;
    display: CurrentDisplay;
    availableStorageBytes: number | null;
    fanLcdRpm: number | null;
    turboPumpRpm: number | null;
    warnings: DeviceWarning[];
    message: string | null;
  };

  type CoolingStatus = Omit<OverviewStatus, "device" | "display">;

  let status = $state<OverviewStatus | null>(null);
  let error = $state<string | null>(null);
  let coolingError = $state<string | null>(null);
  let loading = $state(true);
  let lastTelemetryUpdate = $state<Date | null>(null);
  let dismissedWarningSignature = $state<string | null>(null);
  let activeWarnings = $derived(status?.warnings ?? []);
  let visibleWarnings = $derived(
    activeWarnings.length > 0 && dismissedWarningSignature !== warningSignature(activeWarnings),
  );

  onMount(() => {
    void refreshStatus();
    const timer = window.setInterval(() => {
      void refreshCoolingStatus();
    }, 2_000);

    return () => window.clearInterval(timer);
  });

  async function refreshStatus() {
    loading = true;
    error = null;
    coolingError = null;
    try {
      status = await invoke<OverviewStatus>("overview_status");
      lastTelemetryUpdate = new Date();
    } catch (err) {
      status = null;
      error = err instanceof Error ? err.message : String(err);
    } finally {
      loading = false;
    }
  }

  async function refreshCoolingStatus() {
    try {
      const cooling = await invoke<CoolingStatus>("cooling_status");
      coolingError = null;
      status = {
        daemonAvailable: cooling.daemonAvailable,
        device: status?.device ?? null,
        display: status?.display ?? emptyDisplay(),
        availableStorageBytes: cooling.availableStorageBytes,
        fanLcdRpm: cooling.fanLcdRpm,
        turboPumpRpm: cooling.turboPumpRpm,
        warnings: cooling.warnings,
        message: cooling.message,
      };
      lastTelemetryUpdate = new Date();
    } catch (err) {
      coolingError = err instanceof Error ? err.message : String(err);
    }
  }

  function formatBytes(value: number | null) {
    if (value === null) return "Unavailable";
    return new Intl.NumberFormat(undefined, {
      maximumFractionDigits: 2,
      style: "unit",
      unit: "gigabyte",
      unitDisplay: "short",
    }).format(value / 1_000_000_000);
  }

  function formatRpm(value: number | null) {
    return value === null ? "Unavailable" : `${value.toLocaleString()} RPM`;
  }

  function formatTelemetryTime(value: Date | null) {
    if (!value) return "Waiting for first sample";
    return `Updated ${value.toLocaleTimeString()}`;
  }

  function warningSignature(warnings: DeviceWarning[]) {
    return warnings.map((warning) => `${warning.kind}:${warning.description}`).join("|");
  }

  function dismissWarnings() {
    dismissedWarningSignature = warningSignature(activeWarnings);
  }
</script>

<svelte:head>
  <title>panorama-mgr</title>
</svelte:head>

<main class="shell">
  <Sidebar active="overview" />

  <section class="content">
    <header class="hero">
      <div>
        <p class="eyebrow">Linux desktop control</p>
        <h1>panorama-mgr</h1>
        <p class="summary">
          A focused GUI for TRYX Panorama display control, media preview, and
          device-specific cooling telemetry.
        </p>
      </div>
      <div class="status-card">
        <span class="label">Device</span>
        {#if loading}
          <strong>Checking...</strong>
          <small>Reading daemon IPC status</small>
        {:else if error}
          <strong class="error">Unavailable</strong>
          <small>{error}</small>
        {:else if status?.daemonAvailable && status.device}
          <strong>Connected</strong>
          <small>{status.device.productId} · {status.device.serial}</small>
        {:else}
          <strong class="warning">Daemon offline</strong>
          <small>{status?.message ?? "No daemon status available"}</small>
        {/if}
        <small class="refresh-note">Telemetry syncs every 2 seconds</small>
      </div>
    </header>

    <section class="overview-grid" aria-label="Overview status">
      <article class="panel device-panel">
        <div class="panel-header">
          <h2>Overview</h2>
          <span>{status?.daemonAvailable ? "Daemon IPC online" : "Waiting for daemon"}</span>
        </div>

        <div class="info-grid">
          <div class="info-card">
            <span>Firmware</span>
            <strong>{status?.device?.firmware ?? "Unavailable"}</strong>
          </div>
          <div class="info-card">
            <span>Hardware</span>
            <strong>{status?.device?.hardware ?? "Unavailable"}</strong>
          </div>
          <div class="info-card">
            <span>Available Storage</span>
            <strong>{formatBytes(status?.availableStorageBytes ?? null)}</strong>
          </div>
        </div>

        <DisplaySurface display={status?.display ?? emptyDisplay()} />
      </article>

      <article class="panel telemetry-panel">
        <div class="panel-header">
          <h2>Device Cooling</h2>
          <span>{formatTelemetryTime(lastTelemetryUpdate)}</span>
        </div>

        {#if coolingError}
          <div class="inline-error">{coolingError}</div>
        {/if}

        <div class="metric-list">
          <div class="metric-row">
            <span>Display Fan</span>
            <strong>{formatRpm(status?.fanLcdRpm ?? null)}</strong>
          </div>
          <div class="metric-row">
            <span>Turbo Pump</span>
            <strong>{formatRpm(status?.turboPumpRpm ?? null)}</strong>
          </div>
        </div>
      </article>
    </section>
  </section>

  {#if visibleWarnings}
    <aside class="warning-toast" role="status" aria-live="polite">
      <div class="toast-header">
        <span>{activeWarnings.length === 1 ? "Device warning" : `${activeWarnings.length} device warnings`}</span>
        <button type="button" aria-label="Dismiss warnings" onclick={dismissWarnings}>Dismiss</button>
      </div>
      <ul>
        {#each activeWarnings as warning}
          <li><strong>{warning.kind}</strong>{warning.description}</li>
        {/each}
      </ul>
    </aside>
  {/if}
</main>

<style>
  :global(*) {
    box-sizing: border-box;
  }

  :global(:root) {
    --bg: #070808;
    --surface: rgba(19, 22, 21, 0.9);
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

  button {
    font: inherit;
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
    display: flex;
    justify-content: space-between;
    gap: 32px;
    align-items: flex-start;
    padding-bottom: 34px;
    border-bottom: 1px solid var(--line);
  }

  .eyebrow {
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

  h2 {
    margin-bottom: 0;
    font-size: 20px;
  }

  .summary {
    max-width: 680px;
    color: var(--muted);
    font-size: 18px;
    line-height: 1.6;
  }

  .status-card,
  .panel,
  .info-card {
    border: 1px solid var(--line);
    border-radius: 28px;
    background: linear-gradient(145deg, rgba(24, 27, 26, 0.82), rgba(11, 13, 12, 0.86));
    box-shadow: 0 24px 80px rgba(0, 0, 0, 0.32);
  }

  .status-card {
    min-width: 300px;
    padding: 22px;
  }

  .label,
  .status-card small,
  .panel-header span,
  .info-card span,
  .metric-row span {
    color: var(--muted);
    font-size: 13px;
  }

  .status-card strong {
    display: block;
    margin: 8px 0 6px;
    font-size: 24px;
  }

  .error {
    color: var(--danger);
  }

  .warning {
    color: var(--warning-color);
  }

  .refresh-note {
    display: block;
    margin-top: 18px;
  }

  .inline-error {
    margin-bottom: 14px;
    padding: 12px 14px;
    border: 1px solid rgba(251, 113, 133, 0.34);
    border-radius: 14px;
    background: rgba(251, 113, 133, 0.1);
    color: #fda4af;
    font-size: 13px;
  }

  .overview-grid {
    display: grid;
    grid-template-columns: minmax(720px, 1fr) 300px;
    gap: 24px;
    margin-top: 34px;
  }

  .panel {
    padding: 24px;
  }

  .panel-header {
    display: flex;
    justify-content: space-between;
    gap: 18px;
    align-items: baseline;
    margin-bottom: 22px;
  }

  .info-grid {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: 14px;
    margin-bottom: 24px;
  }

  .info-card {
    padding: 16px;
    box-shadow: none;
  }

  .info-card strong {
    display: block;
    margin-top: 8px;
    overflow: hidden;
    font-size: 18px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .metric-list {
    display: grid;
    gap: 14px;
  }

  .metric-row {
    display: flex;
    justify-content: space-between;
    gap: 18px;
    align-items: center;
    padding: 16px;
    border-radius: 16px;
    background: var(--surface-soft);
  }

  .metric-row strong {
    color: var(--accent-strong);
  }

  .warning-toast {
    position: fixed;
    right: 28px;
    bottom: 28px;
    z-index: 20;
    width: min(420px, calc(100vw - 56px));
    padding: 16px;
    border: 1px solid rgba(251, 191, 36, 0.42);
    border-radius: 22px;
    background: rgba(31, 24, 12, 0.94);
    box-shadow: 0 24px 80px rgba(0, 0, 0, 0.44);
    color: var(--warning-color);
  }

  .toast-header {
    display: flex;
    justify-content: space-between;
    gap: 16px;
    align-items: center;
    margin-bottom: 12px;
    font-size: 13px;
    font-weight: 800;
    letter-spacing: 0.08em;
    text-transform: uppercase;
  }

  .toast-header button {
    border: 0;
    border-radius: 999px;
    padding: 7px 10px;
    background: rgba(251, 191, 36, 0.16);
    color: var(--warning-color);
    cursor: pointer;
    font-size: 12px;
    font-weight: 800;
  }

  .warning-toast ul {
    display: grid;
    gap: 10px;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .warning-toast li {
    display: grid;
    gap: 4px;
    padding: 10px 12px;
    border-radius: 14px;
    background: rgba(251, 191, 36, 0.12);
    color: var(--warning-color);
  }
</style>
