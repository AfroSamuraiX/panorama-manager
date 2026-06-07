<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import DisplaySurface from "$lib/components/DisplaySurface.svelte";
  import Sidebar from "$lib/components/Sidebar.svelte";
  import { emptyDisplay, type CurrentDisplay } from "$lib/display";
  import { onMount } from "svelte";

  type DeviceInfo = {
    productId: string;
    serial: string;
  };

  type DisplayStatus = {
    daemonAvailable: boolean;
    device: DeviceInfo | null;
    display: CurrentDisplay;
    message: string | null;
  };

  type MediaList = {
    files: string[];
    message: string | null;
  };

  type MediaPreview = {
    filename: string;
    previewSrc: string | null;
    message: string | null;
  };

  type ApplyDisplayResult = {
    display: CurrentDisplay;
    message: string;
  };

  let status = $state<DisplayStatus | null>(null);
  let draft = $state<CurrentDisplay>(emptyDisplay());
  let mediaFiles = $state<string[]>([]);
  let mediaMessage = $state<string | null>(null);
  let previewMessage = $state<string | null>(null);
  let loadingPreviewPane = $state<1 | 2 | null>(null);
  let applying = $state(false);
  let applyMessage = $state<string | null>(null);
  let applyError = $state<string | null>(null);
  let loading = $state(true);
  let error = $state<string | null>(null);
  let validationMessages = $derived(validateDraft(draft));
  let hasChanges = $derived(status ? displaySignature(draft) !== displaySignature(status.display) : false);

  const metricOptions = [
    "CPU Temperature",
    "CPU Usage",
    "CPU Frequency",
    "GPU Temperature",
    "GPU Usage",
    "GPU Frequency",
    "GPU Voltage",
    "Memory Utilization",
    "Date&Time",
  ];

  const badgeOptions = ["CPU Badge", "GPU Badge"];

  onMount(() => {
    void refreshStatus();
    void refreshMediaList();
    return () => {};
  });

  async function refreshStatus() {
    loading = true;
    error = null;
    try {
      const next = await invoke<DisplayStatus>("overview_status");
      status = next;
      draft = cloneDisplay(next.display);
    } catch (err) {
      status = null;
      error = err instanceof Error ? err.message : String(err);
    } finally {
      loading = false;
    }
  }

  async function refreshMediaList() {
    try {
      const media = await invoke<MediaList>("media_list");
      mediaFiles = media.files;
      mediaMessage = media.message;
    } catch (err) {
      mediaFiles = [];
      mediaMessage = err instanceof Error ? err.message : String(err);
    }
  }

  function cloneDisplay(display: CurrentDisplay): CurrentDisplay {
    return {
      ...display,
      pane1Media: [...display.pane1Media],
      pane2Media: [...display.pane2Media],
      pane1Metrics: [...display.pane1Metrics],
      pane2Metrics: [...display.pane2Metrics],
      pane1Badges: [...display.pane1Badges],
      pane2Badges: [...display.pane2Badges],
    };
  }

  function setMode(mode: string) {
    draft.screenMode = mode;
    draft.ratio = mode === "Screen Splitting" ? "1:1" : "2:1";
    if (mode !== "Screen Splitting") {
      draft.pane2Media = [];
      draft.pane2PreviewSrc = null;
      draft.pane2Metrics = [];
      draft.pane2Badges = [];
    }
  }

  async function setPaneMedia(pane: 1 | 2, value: string) {
    const trimmed = value.trim();
    const current = status?.display;
    const media = trimmed ? [trimmed] : [];

    if (pane === 1) {
      draft.pane1Media = media;
      draft.pane1PreviewSrc = trimmed === current?.pane1Media[0] ? current.pane1PreviewSrc : null;
    } else {
      draft.pane2Media = media;
      draft.pane2PreviewSrc = trimmed === current?.pane2Media[0] ? current.pane2PreviewSrc : null;
    }

    if (!trimmed || trimmed === (pane === 1 ? current?.pane1Media[0] : current?.pane2Media[0])) {
      previewMessage = null;
      return;
    }

    loadingPreviewPane = pane;
    previewMessage = null;
    try {
      const preview = await invoke<MediaPreview>("media_preview", { filename: trimmed });
      if (preview.filename !== trimmed) return;
      if (pane === 1) {
        draft.pane1PreviewSrc = preview.previewSrc;
      } else {
        draft.pane2PreviewSrc = preview.previewSrc;
      }
      previewMessage = preview.message;
    } catch (err) {
      previewMessage = err instanceof Error ? err.message : String(err);
    } finally {
      loadingPreviewPane = null;
    }
  }

  function paneMediaValue(pane: 1 | 2) {
    return pane === 1 ? (draft.pane1Media[0] ?? "") : (draft.pane2Media[0] ?? "");
  }

  function toggleMetric(pane: 1 | 2, label: string) {
    const key = pane === 1 ? "pane1Metrics" : "pane2Metrics";
    const values = draft[key];
    if (values.includes(label)) {
      draft[key] = values.filter((value) => value !== label);
      return;
    }
    if (values.length >= 3) return;
    draft[key] = [...values, label];
  }

  function metricSelected(pane: 1 | 2, label: string) {
    return (pane === 1 ? draft.pane1Metrics : draft.pane2Metrics).includes(label);
  }

  function toggleBadge(pane: 1 | 2, badge: string) {
    const key = pane === 1 ? "pane1Badges" : "pane2Badges";
    const values = draft[key];
    draft[key] = values.includes(badge)
      ? values.filter((value) => value !== badge)
      : [...values, badge];
  }

  function badgeSelected(pane: 1 | 2, badge: string) {
    return (pane === 1 ? draft.pane1Badges : draft.pane2Badges).includes(badge);
  }

  async function applyDraft() {
    if (!hasChanges || validationMessages.length || applying) return;
    applying = true;
    applyMessage = null;
    applyError = null;
    try {
      const result = await invoke<ApplyDisplayResult>("apply_display", { draft });
      draft = cloneDisplay(result.display);
      if (status) {
        status.display = cloneDisplay(result.display);
      }
      applyMessage = result.message;
    } catch (err) {
      applyError = err instanceof Error ? err.message : String(err);
    } finally {
      applying = false;
    }
  }

  function revertDraft() {
    if (!status) return;
    draft = cloneDisplay(status.display);
    previewMessage = null;
    applyMessage = null;
    applyError = null;
  }

  function validateDraft(display: CurrentDisplay) {
    const messages: string[] = [];
    if (display.screenMode === "Screen Splitting") {
      if (!display.pane1Media.length && !display.pane2Media.length) {
        messages.push("Split mode needs media in at least one pane.");
      }
    } else if (!display.pane1Media.length) {
      messages.push("Full screen mode needs pane 1 media.");
    }
    if (display.pane1Metrics.length > 3 || display.pane2Metrics.length > 3) {
      messages.push("Each pane can render at most 3 metrics.");
    }
    return messages;
  }

  function displaySignature(display: CurrentDisplay) {
    return JSON.stringify({
      screenMode: display.screenMode,
      ratio: display.ratio,
      pane1Media: display.pane1Media,
      pane2Media: display.pane2Media,
      pane1Metrics: display.pane1Metrics,
      pane2Metrics: display.pane2Metrics,
      pane1Badges: display.pane1Badges,
      pane2Badges: display.pane2Badges,
      metricsAlign: display.metricsAlign,
      metricsPosition: display.metricsPosition,
      displayFilter: display.displayFilter,
    });
  }
</script>

<svelte:head>
  <title>panorama-mgr · Display</title>
</svelte:head>

<main class="shell">
  <Sidebar active="display" />

  <section class="content">
    <header class="hero">
      <div>
        <p class="eyebrow">Display workspace</p>
        <h1>Display</h1>
        <p class="summary">
          Shared preview surface used by Overview. Editing controls land next on this page.
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
        <small class="note">Snapshot of the currently running display</small>
      </div>
    </header>

    <section class="display-workspace">
      <section class="panel preview-panel">
        <div class="panel-header">
          <h2>Preview</h2>
          <span>Local draft only</span>
        </div>
        <DisplaySurface display={draft} />
      </section>

      <aside class="panel controls-panel">
        <div class="panel-header">
          <h2>Edit Draft</h2>
          <span>No device writes yet</span>
        </div>

        <label class="field">
          <span>Mode</span>
          <select value={draft.screenMode} onchange={(event) => setMode(event.currentTarget.value)}>
            <option>Full Screen</option>
            <option>Screen Splitting</option>
          </select>
        </label>

        <label class="field">
          <span>Pane 1 Media</span>
          <select value={paneMediaValue(1)} onchange={(event) => void setPaneMedia(1, event.currentTarget.value)}>
            <option value="">No media selected</option>
            {#each mediaFiles as file}
              <option value={file}>{file}</option>
            {/each}
          </select>
        </label>

        {#if draft.screenMode === "Screen Splitting"}
          <label class="field">
            <span>Pane 2 Media</span>
            <select value={paneMediaValue(2)} onchange={(event) => void setPaneMedia(2, event.currentTarget.value)}>
              <option value="">Dark pane</option>
              {#each mediaFiles as file}
                <option value={file}>{file}</option>
              {/each}
            </select>
          </label>
        {/if}

        {#if mediaMessage}
          <div class="inline-note">{mediaMessage}</div>
        {/if}

        {#if loadingPreviewPane}
          <div class="inline-note">Loading pane {loadingPreviewPane} preview...</div>
        {:else if previewMessage}
          <div class="inline-note">{previewMessage}</div>
        {/if}

        <section class="draft-state">
          <div>
            <span>Status</span>
            <strong>{hasChanges ? "Unsaved draft" : "Matches current display"}</strong>
          </div>
          {#if validationMessages.length}
            <ul>
              {#each validationMessages as message}
                <li>{message}</li>
              {/each}
            </ul>
          {:else}
            <small>Draft is valid for a future apply step.</small>
          {/if}
          {#if applyMessage}
            <small class="success-text">{applyMessage}</small>
          {/if}
          {#if applyError}
            <small class="error-text">{applyError}</small>
          {/if}
          <button
            type="button"
            disabled={!hasChanges || validationMessages.length > 0 || applying}
            onclick={applyDraft}
          >
            {applying ? "Applying..." : "Apply Display"}
          </button>
          <button class="secondary-action" type="button" disabled={!hasChanges || applying} onclick={revertDraft}>
            Revert Draft
          </button>
        </section>

        <div class="field-grid">
          <label class="field">
            <span>Overlay Color</span>
            <input bind:value={draft.metricsColor} type="color" />
          </label>
          <label class="field">
            <span>Overlay Align</span>
            <select bind:value={draft.metricsAlign}>
              <option>Left</option>
              <option>Center</option>
              <option>Right</option>
            </select>
          </label>
          <label class="field">
            <span>Overlay Position</span>
            <select bind:value={draft.metricsPosition}>
              <option>Top</option>
              <option>Center</option>
              <option>Bottom</option>
            </select>
          </label>
        </div>

        <label class="field">
          <span>Filter</span>
          <select
            value={draft.displayFilter ?? ""}
            onchange={(event) => (draft.displayFilter = event.currentTarget.value || null)}
          >
            <option value="">None</option>
            <option>Smoke</option>
            <option>Rain</option>
          </select>
        </label>

        <section class="metric-picker">
          <div class="metric-heading">
            <span>Pane 1 Metrics</span>
            <small>{draft.pane1Metrics.length}/3</small>
          </div>
          <div class="metric-options">
            {#each metricOptions as metric}
              <button
                class:active={metricSelected(1, metric)}
                type="button"
                onclick={() => toggleMetric(1, metric)}
              >
                {metric}
              </button>
            {/each}
          </div>
        </section>

        <section class="metric-picker">
          <div class="metric-heading">
            <span>Pane 1 Badges</span>
            <small>{draft.pane1Badges.length}/2</small>
          </div>
          <div class="metric-options">
            {#each badgeOptions as badge}
              <button
                class:active={badgeSelected(1, badge)}
                type="button"
                onclick={() => toggleBadge(1, badge)}
              >
                {badge}
              </button>
            {/each}
          </div>
        </section>

        {#if draft.screenMode === "Screen Splitting"}
          <section class="metric-picker">
            <div class="metric-heading">
              <span>Pane 2 Metrics</span>
              <small>{draft.pane2Metrics.length}/3</small>
            </div>
            <div class="metric-options">
              {#each metricOptions as metric}
                <button
                  class:active={metricSelected(2, metric)}
                  type="button"
                  onclick={() => toggleMetric(2, metric)}
                >
                  {metric}
                </button>
              {/each}
            </div>
          </section>

          <section class="metric-picker">
            <div class="metric-heading">
              <span>Pane 2 Badges</span>
              <small>{draft.pane2Badges.length}/2</small>
            </div>
            <div class="metric-options">
              {#each badgeOptions as badge}
                <button
                  class:active={badgeSelected(2, badge)}
                  type="button"
                  onclick={() => toggleBadge(2, badge)}
                >
                  {badge}
                </button>
              {/each}
            </div>
          </section>
        {/if}
      </aside>
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
    font-size: clamp(36px, 5vw, 56px);
    letter-spacing: -0.06em;
  }

  .summary {
    max-width: 720px;
    color: var(--muted);
    font-size: 18px;
    line-height: 1.6;
  }

  .status-card,
  .panel {
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
  .panel-header span {
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

  .note {
    display: block;
    margin-top: 18px;
  }

  .panel {
    margin-top: 24px;
    padding: 24px;
    max-width: 1180px;
  }

  .display-workspace {
    display: grid;
    grid-template-columns: minmax(640px, 1fr) 380px;
    gap: 24px;
    align-items: start;
  }

  .display-workspace .panel {
    max-width: none;
  }

  .preview-panel {
    min-width: 0;
  }

  .controls-panel {
    display: grid;
    gap: 18px;
  }

  .panel-header {
    display: flex;
    justify-content: space-between;
    gap: 18px;
    align-items: baseline;
    margin-bottom: 22px;
  }

  .field,
  .metric-picker {
    display: grid;
    gap: 8px;
  }

  .field span,
  .metric-heading span,
  .metric-heading small {
    color: var(--muted);
    font-size: 13px;
  }

  input,
  select {
    width: 100%;
    border: 1px solid var(--line);
    border-radius: 14px;
    padding: 11px 12px;
    background: #111412;
    color: var(--text);
    font: inherit;
    color-scheme: dark;
  }

  input[type="color"] {
    min-height: 44px;
    padding: 5px;
  }

  select:focus {
    border-color: rgba(158, 240, 184, 0.46);
    outline: none;
  }

  option {
    background: #111412;
    color: var(--text);
  }

  .inline-note {
    padding: 11px 12px;
    border: 1px solid rgba(240, 184, 74, 0.28);
    border-radius: 14px;
    background: rgba(240, 184, 74, 0.08);
    color: var(--warning-color);
    font-size: 13px;
  }

  .draft-state {
    display: grid;
    gap: 12px;
    padding: 14px;
    border: 1px solid rgba(158, 240, 184, 0.16);
    border-radius: 18px;
    background: rgba(242, 240, 234, 0.045);
  }

  .draft-state span,
  .draft-state small,
  .draft-state li {
    color: var(--muted);
    font-size: 13px;
  }

  .draft-state strong {
    display: block;
    margin-top: 5px;
    color: var(--accent-strong);
  }

  .draft-state ul {
    display: grid;
    gap: 6px;
    margin: 0;
    padding-left: 18px;
  }

  .success-text {
    color: var(--accent-strong) !important;
  }

  .error-text {
    color: var(--danger) !important;
  }

  .draft-state button {
    border: 0;
    border-radius: 999px;
    padding: 10px 14px;
    background: linear-gradient(135deg, var(--accent), var(--accent-deep));
    color: #06100b;
    cursor: pointer;
    font: inherit;
    font-weight: 800;
  }

  .draft-state button:disabled {
    cursor: not-allowed;
    filter: grayscale(0.5);
    opacity: 0.58;
  }

  .draft-state .secondary-action {
    border: 1px solid var(--line);
    background: rgba(242, 240, 234, 0.06);
    color: var(--text);
  }

  .field-grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 12px;
  }

  .metric-heading {
    display: flex;
    justify-content: space-between;
    gap: 12px;
  }

  .metric-options {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }

  .metric-options button {
    border: 1px solid rgba(158, 240, 184, 0.18);
    border-radius: 999px;
    padding: 7px 10px;
    background: rgba(242, 240, 234, 0.055);
    color: var(--muted);
    cursor: pointer;
    font: inherit;
    font-size: 12px;
  }

  .metric-options button.active {
    border-color: rgba(158, 240, 184, 0.52);
    background: rgba(158, 240, 184, 0.14);
    color: var(--accent-strong);
  }
</style>
