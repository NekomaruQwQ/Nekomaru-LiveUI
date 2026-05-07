<script lang="ts">
    import { DEBUG } from "../../debug";
    import { ChromaKeyRenderer, parseHexColor } from "./chroma-key";
    import { startStreamLoop } from "./stream-loop";

    type Props = {
        streamId: string;
        chromaKey?: string;
    };

    let { streamId, chromaKey }: Props = $props();

    let canvas: HTMLCanvasElement;

    /// (Re-)mount the stream loop whenever streamId or chromaKey changes.
    /// `$effect` automatically tears down the previous loop before setting up
    /// a new one, mirroring the React useEffect cleanup pattern.
    $effect(() => {
        console.log("StreamRenderer: Component mounted");

        if (!canvas) {
            console.error("StreamRenderer: Canvas ref is null!");
            return;
        }

        // ── Build the frame renderer ─────────────────────────────────────
        // When chroma-key is active, use a WebGL2 shader that keys out the
        // target color.  Otherwise, use a plain 2D canvas drawImage path.
        let onFrame: (frame: VideoFrame) => void;
        let cleanup: (() => void) | undefined;

        if (chromaKey) {
            const renderer = new ChromaKeyRenderer(canvas, parseHexColor(chromaKey));
            onFrame = (frame) => renderer.render(frame);
            cleanup = () => renderer.dispose();
            console.log("StreamRenderer: Using WebGL chroma-key renderer (key=%s)", chromaKey);
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
    class="w-full object-contain {chromaKey ? '' : 'bg-[#1e1f22]'}">
</canvas>
