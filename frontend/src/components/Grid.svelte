<script lang="ts">
    import type { Snippet } from "svelte";

    type Props = {
        gap?: number | string;
        class?: string;
        style?: string;
        children: Snippet;
    } & (
        | { rows: string; columns?: never }
        | { rows?: never; columns: string });

    let {
        rows,
        columns,
        gap,
        class: className = "",
        style = "",
        children,
    }: Props = $props();

    /// The defined axis uses the provided track list; the cross axis fills all space.
    let inlineStyle = $derived(
        `display:grid;`
        + `grid-template-rows:${rows ?? "1fr"};`
        + `grid-template-columns:${columns ?? "1fr"};`
        + style);
</script>

<div class="grid gap-{gap} {className}" style={inlineStyle}>
    {@render children()}
</div>
