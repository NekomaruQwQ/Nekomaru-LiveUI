<script lang="ts">
    import type { Snippet } from "svelte";

    type Props = {
        name: string | Snippet;
        /// Optional icon snippet rendered to the left of the label+content stack.
        /// The parent decides what to render inside (Icon component, raw SVG, emoji, etc.).
        icon?: Snippet;
        class?: string;
        children: Snippet;
    };

    let { name, icon, class: className = "", children }: Props = $props();
</script>

<!--
    Two-row status indicator with an optional icon presenter.

    Layout: icon (left, vertically centered) | label + content stack (right).
    Purely presentational — the parent supplies the icon and dynamic content
    (e.g. from the string store).
-->
<div class="flex! flex-row items-center gap-1 {className}">
    {#if icon}
        <div class="flex! size-10 items-center justify-center shrink-0 opacity-75">
            {@render icon()}
        </div>
    {/if}
    <div class="flex! flex-col">
        <div class="pl-0.5 text-xs">
            {#if typeof name === "string"}{name}{:else}{@render name()}{/if}
        </div>
        {@render children()}
    </div>
</div>
