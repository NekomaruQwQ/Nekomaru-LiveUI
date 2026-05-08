<script lang="ts">
    import { DEBUG } from "../../debug";
    import { ColorKeyRenderer, parseHexColor } from "./color-key";
    import { startStreamLoop } from "./stream-loop";

    type Props = {
        streamId: string;
        /// Hex color (e.g. "#212121") or list of hex colors to key out.
        /// When omitted or empty, the stream renders through a plain 2D canvas.
        colorKey?: string | string[];
        /// Smoothstep knee `[low, high]` over the unspill ratio in [0,1].
        /// `low` is the noise floor (≤ low → fully transparent); `high` is
        /// the solid snap (≥ high → fully opaque).  Falls back to the
        /// renderer's defaults (≈ 0.02 / 0.98) when unset.
        colorKeyKnee?: [number, number];
    };

    let { streamId, colorKey, colorKeyKnee }: Props = $props();

    let canvas: HTMLCanvasElement;

    /// Normalise the prop to a plain array so the rest of the component only
    /// has one shape to think about.  Returns `[]` when nothing is keyed out.
    const keyList = $derived(
        colorKey === undefined ? []
            : typeof colorKey === "string" ? [colorKey]
            : colorKey);

    /// (Re-)mount the stream loop whenever streamId or colorKey changes.
    /// `$effect` automatically tears down the previous loop before setting up
    /// a new one, mirroring the React useEffect cleanup pattern.
    $effect(() => {
        console.log("StreamRenderer: Component mounted");

        if (!canvas) {
            console.error("StreamRenderer: Canvas ref is null!");
            return;
        }

        // ── Build the frame renderer ─────────────────────────────────────
        // When color-key is active, use a WebGL2 shader that keys out the
        // target colors.  Otherwise, use a plain 2D canvas drawImage path.
        let onFrame: (frame: VideoFrame) => void;
        let cleanup: (() => void) | undefined;

        if (keyList.length > 0) {
            // Forward knees as-is; `undefined` slots fall back to the renderer's defaults.
            const renderer = new ColorKeyRenderer(
                canvas, keyList.map(parseHexColor), colorKeyKnee?.[0], colorKeyKnee?.[1]);
            onFrame = (frame) => renderer.render(frame);
            cleanup = () => renderer.dispose();
            console.log("StreamRenderer: Using WebGL color-key renderer (keys=%s, knee=%s)",
                keyList.join(","), colorKeyKnee ? `${colorKeyKnee[0]}..${colorKeyKnee[1]}` : "default");
        } else {
            const ctx = canvas.getContext("2d");
            if (!ctx) {
                console.error("StreamRenderer: Failed to get 2D context");
                return;
            }
            onFrame = (frame) => renderFrame(canvas, ctx, frame);
            console.log("StreamRenderer: Using 2D canvas renderer");
        }

        console.log("StreamRenderer: Canvas ready: %dx%d", canvas.width, canvas.height);

        const abortController = new AbortController();
        void startStreamLoop(streamId, onFrame, abortController.signal);

        return () => {
            console.log("StreamRenderer: Component unmounting, aborting stream loop");
            abortController.abort();
            cleanup?.();
        };
    });

    let lastFrameTime = 0;

    /// Render a decoded video frame to canvas.
    function renderFrame(c: HTMLCanvasElement, ctx: CanvasRenderingContext2D, frame: VideoFrame) {
        if (c.width !== frame.displayWidth || c.height !== frame.displayHeight) {
            c.width = frame.displayWidth;
            c.height = frame.displayHeight;
            console.log(
                "StreamRenderer: Canvas resized to %dx%d",
                frame.displayWidth,
                frame.displayHeight);
        }

        if (DEBUG.debugStreamRenderer) {
            console.log("StreamRenderer: Rendering frame to canvas - timestamp: %d μs", frame.timestamp);
        }
        ctx.drawImage(frame, 0, 0);

        // CRITICAL: Close frame to release GPU memory.
        frame.close();

        if (DEBUG.debugStreamRenderer) {
            console.log("StreamRenderer: Frame closed (GPU memory released)");
        }

        const now = performance.now();
        if (lastFrameTime > 0) {
            const delta = now - lastFrameTime;
            if (DEBUG.debugStreamRenderer) {
                console.log("StreamRenderer: Frame interval: %d ms", delta);
            }
        }
        lastFrameTime = now;
    }
</script>

<canvas
    bind:this={canvas}
    class="w-full object-contain">
</canvas>
