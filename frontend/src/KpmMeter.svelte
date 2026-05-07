<script lang="ts">
    import { onMount } from "svelte";
    import { Keyboard } from "@lucide/svelte";
    import { kpmWsLoop } from "./kpm-loop";

    // ── Constants ────────────────────────────────────────────────────────────

    /// KPM value that maps to 100% bar height.
    const MAX_KPM = 480;

    /// Power curve exponent for the height mapping.
    /// < 1.0 compresses the top end and expands the lower range, making
    /// moderate typing (50-300 KPM) more visually interesting.
    const CURVE_EXPONENT = 0.7;

    /// Peak hold duration before decay begins (ms).
    const PEAK_HOLD_MS = 1500;

    /// Duration of the linear decay from peak to current (ms).
    const PEAK_DECAY_MS = 500;

    // ── State ────────────────────────────────────────────────────────────────

    /// Current KPM and computed peak.  `null` means the process isn't running
    /// (KPM endpoint hasn't yielded a value yet, or returned null).
    let kpm = $state<number | null>(null);
    let peak = $state(0);

    /// Peak tracking — plain `let` (no reactivity) so updates don't trigger
    /// re-renders.  Only the `kpm` / `peak` $state writes drive the DOM.
    let peakRaw = 0;
    let peakTime = 0;

    onMount(() => {
        const abort = new AbortController();

        void kpmWsLoop(abort.signal, (next) => {
            if (next == null) { kpm = null; peak = 0; return; }

            const now = performance.now();

            if (next >= peakRaw) {
                peakRaw = next;
                peakTime = now;
            } else {
                const elapsed = now - peakTime;
                if (elapsed > PEAK_HOLD_MS) {
                    const decayProgress = Math.min(
                        (elapsed - PEAK_HOLD_MS) / PEAK_DECAY_MS, 1);
                    peakRaw = peakRaw + (next - peakRaw) * decayProgress;
                }
            }

            kpm = next;
            peak = Math.round(peakRaw);
        });

        return () => abort.abort();
    });

    // ── Derived display values ───────────────────────────────────────────────

    /// Map a KPM value to a 0–100 percentage using a power curve.
    function kpmToPercent(value: number): number {
        const clamped = Math.min(Math.max(value, 0), MAX_KPM);
        return (clamped / MAX_KPM) ** CURVE_EXPONENT * 100;
    }

    let barPercent = $derived(kpm == null ? 0 : kpmToPercent(kpm));
    let peakPercent = $derived(kpmToPercent(peak));
</script>

<!--
    Vertical VU-style KPM meter with peak hold marker.

    Renders nothing if the KPM endpoint returns 404 (process not running).
    At zero KPM, shows an empty meter ("quiet studio" aesthetic).
-->
{#if kpm != null}
    <div class="flex! flex-col items-center w-full h-full gap-1">
        <!-- Meter body -->
        <div class="kpm-meter flex-1 w-full relative">
            <!-- Realtime bar — lower visual weight.  The LED segment look is
                 baked into the bar's own background, so the gaps only appear
                 where the bar is filled (no stray lines over empty space). -->
            <div
                class="kpm-bar absolute inset-x-0 bottom-0 rounded-sm"
                style="height: {barPercent}%">
            </div>

            <!-- Peak hold marker — the hero element -->
            {#if peak > 0}
                <div
                    class="kpm-peak absolute inset-x-0"
                    style="bottom: {peakPercent}%">
                    <span class="kpm-peak-label">{peak}</span>
                </div>
            {/if}
        </div>

        <!-- Readout + label area -->
        <div class="flex! flex-col items-center gap-0.5 shrink-0">
            <span class="text-sm font-light opacity-75">{kpm}</span>
            <span class="text-[10px] tracking-wider font-light opacity-50">KPM</span>
            <Keyboard size={24} class="opacity-50" />
        </div>
    </div>
{/if}

<style>
    /* Vertical LED-style meter for keystrokes-per-minute.  Uses a single neon
       accent color to avoid competing with the live capture for visual attention.

       COLOR STRATEGY: `.island` sets `color: var(--theme-color)`, which inherits
       to all children as a resolved color value.  Every KPM element below uses
       `currentColor` so it automatically picks up the island's theme color
       without each one having to reference `--theme-color` directly.

       Two layers inside `.kpm-meter`:
         1. .kpm-bar   — the realtime value, drawn as a striped pattern so the
                         LED segment gaps only appear where the bar is filled
                         (lower opacity, background role)
         2. .kpm-peak  — peak hold marker (the hero — bright, with glow)

       The bar height and peak position are set via inline `style.height` /
       `style.bottom` from the component — CSS handles the smooth transitions.
       ────────────────────────────────────────────────────────────────────────── */

    .kpm-meter {
        border-radius: 4px;
        overflow: hidden;
        box-shadow: inset 0 0 8px color-mix(in oklch, currentColor 10%, transparent);
    }

    /* Realtime KPM bar — semi-transparent to stay visually subdued.
       The bar is the background actor; the peak marker is the star.

       The repeating gradient draws the bar as 3px-tall stripes separated by
       1px transparent gaps, producing the classic LED segment look without
       a separate overlay layer (which would otherwise leak lines into the
       empty area above the bar). */
    .kpm-bar {
        background-image: repeating-linear-gradient(
            to top,
            color-mix(in oklch, currentColor 40%, transparent) 0px,
            color-mix(in oklch, currentColor 40%, transparent) 3px,
            transparent 3px,
            transparent 4px
        );
        transition: height 200ms linear;
        z-index: 1;
    }

    /* Peak hold marker — the hero element that chat reads.
       A bright horizontal line with a neon glow, positioned at the peak. */
    .kpm-peak {
        height: 3px;
        background: currentColor;
        box-shadow:
            0 0 6px color-mix(in oklch, currentColor 60%, transparent),
            0 0 12px color-mix(in oklch, currentColor 30%, transparent);
        transition: bottom 200ms linear;
        z-index: 3;
        /* Position the label relative to this marker */
        display: flex;
        align-items: center;
        justify-content: center;
    }

    /* KPM number floating near the peak marker */
    .kpm-peak-label {
        position: absolute;
        right: calc(100% + 4px);
        font-size: 10px;
        font-weight: 600;
        color: currentColor;
        text-shadow: 0 0 6px color-mix(in oklch, currentColor 50%, transparent);
        white-space: nowrap;
        pointer-events: none;
    }
</style>
