<script lang="ts">
    import Icon from "@/components/Icon.svelte";
    import LiveWidget from "./LiveWidget.svelte";
    import { strings } from "@/strings.svelte";

    let captureMode = $derived(strings.value.$captureMode?.toUpperCase() ?? "—");
    let captureInfo = $derived(strings.value.$captureInfo ?? "");
</script>

<!-- Large widget showing the current capture target and mode (AUTO/LOCKED). -->
<LiveWidget name={`Capture Mode - ${captureMode}`}>
    {#snippet icon()}
        <Icon name="monitor" size={36} />
    {/snippet}
    <span class="text-sm truncate">
        {#if captureInfo} {captureInfo}{/if}
    </span>
</LiveWidget>
