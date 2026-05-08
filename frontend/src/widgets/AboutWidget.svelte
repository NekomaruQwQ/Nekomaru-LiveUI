<script lang="ts">
    import { Info } from "@lucide/svelte";
    import LiveWidget from "./LiveWidget.svelte";
    import { strings } from "@/events.svelte";

    const timestamp = $derived((strings.value.$timestamp ?? "").slice(0, 16));
</script>

<!-- Large widget anchored to the bottom of the SidePanel.  Shows the
     revision timestamp and credits. -->
<LiveWidget name={`Nekomaru LiveUI - Build ${timestamp}`} class="text-xs">
    {#snippet icon()}
        <Info size={40} />
    {/snippet}
    <span class="opacity-75 leading-tight">Designed & Created by NekomaruQwQ</span>
    <span class="opacity-75 leading-tight">Coauthored by Claude Code</span>
</LiveWidget>
