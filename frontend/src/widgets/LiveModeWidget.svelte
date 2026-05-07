<script lang="ts">
    import Icon from "@/components/Icon.svelte";
    import LiveWidget from "./LiveWidget.svelte";
    import { strings } from "@/strings.svelte";

    /// Display labels and icons for each mode value.
    const MODE_MAP = {
        unknown: { label: "—", icon: "activity" },
        code: { label: "Coding", icon: "code" },
        game: { label: "Gaming", icon: "gamepad" },
        sing: { label: "Singing", icon: "music" },
        chat: { label: "Chatting", icon: "message-circle" },
        brb: { label: "BRB", icon: "coffee" },
    } as const;

    let mode = $derived(
        (strings.value.$liveMode
            && MODE_MAP[strings.value.$liveMode as keyof typeof MODE_MAP])
            || MODE_MAP.unknown);
</script>

<!-- Shows the current live mode derived from the auto-selector's `@mode` tag. -->
<LiveWidget name="Live Mode">
    {#snippet icon()}
        <Icon name={mode.icon} size={36} />
    {/snippet}
    <span class="text-sm">{mode.label}</span>
</LiveWidget>
