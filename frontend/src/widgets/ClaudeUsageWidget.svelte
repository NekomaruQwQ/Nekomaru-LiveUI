<script lang="ts">
    import Icon from "@/components/Icon.svelte";
    import LiveWidget from "./LiveWidget.svelte";
    import { strings } from "@/events.svelte";

    // `$claudeTokens` / `$claudeCost` are plain numeric strings posted by the
    // `run-ccusage --loop` Nushell launcher every minute.  Parse defensively:
    // an absent key (poller not running, or first iteration still in flight)
    // renders as a dash placeholder rather than "0.0 M ($0.0)" which would
    // falsely imply the totals are actually zero.
    const tokens = $derived(Number(strings.value.$claudeTokens ?? NaN));
    const cost   = $derived(Number(strings.value.$claudeCost   ?? NaN));

    const display = $derived(
        Number.isFinite(tokens) && Number.isFinite(cost)
            ? `${(tokens / 1e6).toFixed(1)} M tokens (${cost.toFixed(1)} USD)`
            : "—");
</script>

<!-- Today's Claude Code token usage + estimated cost, polled by run-ccusage. -->
<LiveWidget name="Estimated Token Usage Today">
    {#snippet icon()}
        <Icon name="circle-dollar-sign" size={40} />
    {/snippet}
    <span class="text-lg">{display}</span>
</LiveWidget>
