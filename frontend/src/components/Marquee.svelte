<script lang="ts">
    import { onMount } from "svelte";

    /// Pixels per second — consistent reading speed regardless of text length.
    const MARQUEE_SPEED = 30;

    let { text }: { text: string } = $props();

    let spanEl: HTMLSpanElement;
    let divEl: HTMLDivElement;

    /// Measure one copy's rendered width → set animationDuration directly on
    /// the DOM element.  Avoids reactive churn that would restart the animation.
    /// ResizeObserver re-fires when the text content changes.
    onMount(() => {
        let prevDuration = "";
        const measure = () => {
            const next = `${spanEl.offsetWidth / MARQUEE_SPEED}s`;
            if (next !== prevDuration) {
                prevDuration = next;
                divEl.style.animationDuration = next;
            }
        };

        const observer = new ResizeObserver(measure);
        observer.observe(spanEl);
        measure();
        return () => observer.disconnect();
    });

    /// En-space + middle dot + en-space, matching the React version.
    let item = $derived(`${text} · `);
</script>

<div
    bind:this={divEl}
    class="flex! overflow-visible! w-max flex-row marquee text-sm animate-[marquee_linear_infinite]">
    <span bind:this={spanEl} class="shrink-0 min-w-auto">{item}</span>
    <span class="shrink-0 min-w-auto">{item}</span>
</div>
