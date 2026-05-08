<script lang="ts">
    import Icon from "@/components/Icon.svelte";
    import LiveWidget from "./LiveWidget.svelte";
    import { strings } from "@/events.svelte";

    /// Display labels and icons for each mode value.
    const MODE_MAP = {
        unknown: { label: "—", icon: "activity" },
        code: { label: "Coding", icon: "bug" },
        game: { label: "Gaming", icon: "gamepad" },
        sing: { label: "Singing", icon: "music" },
        chat: { label: "Chatting", icon: "message-circle" },
        brb: { label: "BRB", icon: "coffee" },
    } as const;

    let mode = $derived(
        (strings.value.$liveMode
            && MODE_MAP[strings.value.$liveMode as keyof typeof MODE_MAP])
            || MODE_MAP.unknown);
    let captureMode = $derived(strings.value.$captureMode?.toUpperCase() ?? "UNKNOWN");
    let captureInfo = $derived(strings.value.$captureInfo ?? "");
</script>

<!-- Shows the current live mode derived from the auto-selector's `@mode` tag. -->
<LiveWidget name={`Live Capture - ${captureMode}`}>
    {#snippet icon()}
        <Icon name={mode.icon} size={40} />
    {/snippet}
    <span class="text-sm">{mode.label} - {captureInfo}</span>
</LiveWidget>
