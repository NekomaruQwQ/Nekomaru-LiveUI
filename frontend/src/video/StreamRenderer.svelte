<script lang="ts" module>
    export type StreamRendererProps = {
        streamId: string;
        /// Hex color (e.g. "#212121") or list of hex colors to key out.
        /// When omitted or empty the shader degenerates to a sRGB-correct
        /// passthrough — same WebGL2 path either way, so the canvas's
        /// context type stays stable across prop changes.
        colorKey?: string | string[];
        /// Smoothstep knee `[low, high]` over the unspill ratio in [0,1].
        /// `low` is the noise floor (≤ low → fully transparent); `high` is
        /// the solid snap (≥ high → fully opaque).  Falls back to the
        /// renderer's defaults (≈ 0.02 / 0.98) when unset.
        colorKeyKnee?: [number, number];
        /// Hex sRGB color (e.g. "#FF00FF") that replaces the kept-pixel RGB
        /// while preserving the keyer's soft alpha — useful for solid-color
        /// silhouettes.  Without a `colorKey` the entire frame is opaque and
        /// becomes a flat fill of this color, which is rarely what you want.
        binarizationColor?: string;
    };
</script>

<script lang="ts">
    import { ColorKeyRenderer, parseHexColor } from "./color-key";
    import { startStreamLoop } from "./stream-loop";

    let { streamId, colorKey, colorKeyKnee, binarizationColor }: StreamRendererProps = $props();

    let canvas: HTMLCanvasElement;

    /// Normalise the prop to a plain array so the rest of the component only
    /// has one shape to think about.  Returns `[]` when nothing is keyed out.
    const keyList = $derived(
        colorKey === undefined ? []
            : typeof colorKey === "string" ? [colorKey]
            : colorKey);

    /// (Re-)mount the stream loop whenever streamId or any renderer prop
    /// changes.  `$effect` tears down the previous loop and ColorKeyRenderer
    /// before setting up a new one.
    $effect(() => {
        if (!canvas) {
            console.error("StreamRenderer: Canvas ref is null!");
            return;
        }

        // Always go through WebGL2 — even with no keys the shader is a
        // passthrough.  Mixing 2D and WebGL2 contexts on the same canvas
        // node is impossible (a canvas binds to exactly one context kind
        // for its DOM lifetime), so we commit to WebGL2 up front.
        const renderer = new ColorKeyRenderer(
            canvas, keyList.map(parseHexColor), colorKeyKnee?.[0], colorKeyKnee?.[1],
            binarizationColor ? parseHexColor(binarizationColor) : undefined);
        console.log("StreamRenderer: Using WebGL color-key renderer (keys=%s, knee=%s, bin=%s)",
            keyList.join(",") || "none",
            colorKeyKnee ? `${colorKeyKnee[0]}..${colorKeyKnee[1]}` : "default",
            binarizationColor ?? "off");

        const abortController = new AbortController();
        void startStreamLoop(streamId, (frame) => renderer.render(frame), abortController.signal);

        return () => {
            console.log("StreamRenderer: Component unmounting, aborting stream loop");
            abortController.abort();
            renderer.dispose();
        };
    });
</script>

<canvas
    bind:this={canvas}
    class="w-full object-contain">
</canvas>
