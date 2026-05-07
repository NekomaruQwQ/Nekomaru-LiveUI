<script lang="ts">
    import { Info } from "@lucide/svelte";
    import LiveWidget from "./LiveWidget.svelte";
    import { strings } from "@/strings.svelte";

    let timestamp = $derived(strings.value.$timestamp ?? "");
</script>

<!-- Large widget anchored to the bottom of the SidePanel.  Shows the
     revision timestamp and credits. -->
<div class="text-xs">
    <LiveWidget name="Nekomaru LiveUI">
        {#snippet icon()}
            <Info size={30} />
        {/snippet}
        <div class="text-xs opacity-50">
            {#if timestamp}<span>Build {timestamp.slice(0, 16)}</span>{/if}
        </div>
    </LiveWidget>
    <div class="px-2">
        <span class="opacity-75">Designed & Created by Nekomaru@pku.edu.cn</span>
        <span class="opacity-50">Coauthored by Claude Code</span>
    </div>
</div>
