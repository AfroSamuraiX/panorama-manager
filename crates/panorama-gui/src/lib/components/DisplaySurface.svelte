<script lang="ts">
  import type { CurrentDisplay } from "$lib/display";

  const { display } = $props<{ display: CurrentDisplay }>();

  function firstMedia(files: string[]) {
    return files[0] ?? "No media selected";
  }

  function formatMetrics(metrics: string[]) {
    return metrics.length ? metrics : ["No metrics"];
  }

  function isImagePreview(filename: string) {
    return /\.(bmp|jpe?g|png|webp)$/i.test(filename);
  }

  function replayAfterPause(event: Event) {
    const video = event.currentTarget;
    if (!(video instanceof HTMLVideoElement)) return;
    window.setTimeout(() => {
      video.currentTime = 0;
      void video.play();
    }, 500);
  }
</script>

<div class:split={display.screenMode === "Screen Splitting"} class="screen-frame">
  <div class="screen-glow"></div>
  {#if display.saved}
    <div class="display-pane">
      {#if display.pane1PreviewSrc}
        {#if isImagePreview(firstMedia(display.pane1Media))}
          <img src={display.pane1PreviewSrc} alt="Current display pane 1 preview" />
        {:else}
          <video
            src={display.pane1PreviewSrc}
            muted
            autoplay
            playsinline
            onended={replayAfterPause}
            aria-label="Current display pane 1 preview"
          ></video>
        {/if}
      {/if}
      <span>Pane 1</span>
      <strong>{firstMedia(display.pane1Media)}</strong>
      <div class="metric-chips">
        {#each formatMetrics(display.pane1Metrics) as metric}
          <small>{metric}</small>
        {/each}
      </div>
      {#if display.pane1Badges.length}
        <div class="badge-chips">
          {#each display.pane1Badges as badge}
            <small>{badge}</small>
          {/each}
        </div>
      {/if}
    </div>
    {#if display.screenMode === "Screen Splitting"}
      <div class="display-pane secondary">
        {#if display.pane2PreviewSrc}
          {#if isImagePreview(firstMedia(display.pane2Media))}
            <img src={display.pane2PreviewSrc} alt="Current display pane 2 preview" />
          {:else}
            <video
              src={display.pane2PreviewSrc}
              muted
              autoplay
              playsinline
              onended={replayAfterPause}
              aria-label="Current display pane 2 preview"
            ></video>
          {/if}
        {/if}
        <span>Pane 2</span>
        <strong>{firstMedia(display.pane2Media)}</strong>
        <div class="metric-chips">
          {#each formatMetrics(display.pane2Metrics) as metric}
            <small>{metric}</small>
          {/each}
        </div>
        {#if display.pane2Badges.length}
          <div class="badge-chips">
            {#each display.pane2Badges as badge}
              <small>{badge}</small>
            {/each}
          </div>
        {/if}
      </div>
    {/if}
  {:else}
    <div class="screen-copy">
      <span>Current Display</span>
      <strong>No saved display yet</strong>
    </div>
  {/if}
</div>

<div class="display-summary" aria-label="Current display summary">
  <div>
    <span>Mode</span>
    <strong>{display.screenMode}</strong>
  </div>
  <div>
    <span>Ratio</span>
    <strong>{display.ratio}</strong>
  </div>
  <div>
    <span>Overlay</span>
    <strong>{display.metricsAlign} · {display.metricsPosition}</strong>
  </div>
  <div>
    <span>Filter</span>
    <strong>{display.displayFilter ?? "None"}</strong>
  </div>
</div>

<style>
  .screen-frame {
    position: relative;
    display: grid;
    grid-template-columns: 1fr;
    gap: 1px;
    width: 100%;
    min-height: 430px;
    aspect-ratio: 2 / 1;
    place-items: center;
    overflow: hidden;
    border: 1px solid rgba(158, 240, 184, 0.42);
    border-radius: 22px;
    background:
      radial-gradient(circle at 20% 20%, rgba(158, 240, 184, 0.18), transparent 28%),
      linear-gradient(135deg, #141716, #050606 70%);
    color: var(--accent-strong);
  }

  .screen-frame.split {
    grid-template-columns: 1fr 1fr;
  }

  .screen-glow {
    position: absolute;
    inset: 0;
    background: linear-gradient(90deg, transparent, rgba(255, 255, 255, 0.08), transparent);
    transform: skewX(-18deg) translateX(-20%);
  }

  .screen-copy {
    position: relative;
    display: grid;
    gap: 6px;
    text-align: center;
  }

  .display-pane {
    position: relative;
    display: grid;
    gap: 10px;
    align-content: center;
    width: 100%;
    height: 100%;
    padding: 24px;
    background: rgba(255, 255, 255, 0.02);
    text-align: left;
  }

  .display-pane::before {
    position: absolute;
    inset: 0;
    z-index: 1;
    background: linear-gradient(90deg, rgba(5, 6, 6, 0.76), rgba(5, 6, 6, 0.22));
    content: "";
  }

  .display-pane img,
  .display-pane video {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    object-fit: cover;
  }

  .display-pane.secondary {
    border-left: 1px solid rgba(158, 240, 184, 0.24);
    background: rgba(255, 255, 255, 0.035);
  }

  .screen-copy span,
  .display-pane span,
  .display-summary span {
    color: var(--muted);
    font-size: 13px;
    font-weight: 800;
    letter-spacing: 0.14em;
    text-transform: uppercase;
  }

  .display-pane span,
  .display-pane strong,
  .display-pane .metric-chips,
  .display-pane .badge-chips {
    position: relative;
    z-index: 2;
  }

  .screen-copy strong {
    font-size: 24px;
  }

  .display-pane strong {
    overflow: hidden;
    color: var(--text);
    font-size: 18px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .metric-chips {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }

  .badge-chips {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }

  .badge-chips small {
    border: 1px solid rgba(240, 184, 74, 0.22);
    border-radius: 999px;
    padding: 5px 8px;
    background: rgba(240, 184, 74, 0.1);
    color: var(--warning-color);
    font-size: 12px;
  }

  .metric-chips small {
    border: 1px solid rgba(158, 240, 184, 0.18);
    border-radius: 999px;
    padding: 5px 8px;
    background: rgba(158, 240, 184, 0.08);
    color: var(--accent-strong);
    font-size: 12px;
  }

  .display-summary {
    display: grid;
    grid-template-columns: repeat(4, minmax(0, 1fr));
    gap: 12px;
    margin-top: 14px;
  }

  .display-summary div {
    padding: 13px 14px;
    border-radius: 16px;
    background: var(--surface-soft);
  }

  .display-summary strong {
    display: block;
    margin-top: 6px;
    overflow: hidden;
    color: var(--text);
    font-size: 14px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
