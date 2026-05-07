<script lang="ts" module>
    import type { Component } from "svelte";
    import {
        Activity,
        Clock1, Clock2, Clock3, Clock4, Clock5, Clock6,
        Clock7, Clock8, Clock9, Clock10, Clock11, Clock12,
        Code,
        Coffee,
        Gamepad,
        MessageCircle,
        Mic, MicOff,
        Monitor,
        Music,
    } from "@lucide/svelte";

    /// Static icon map.  Mirrors the React version's lookup table — adding a new
    /// icon means importing its PascalCase component above and adding a kebab-case
    /// entry here.
    const ICONS = {
        "activity": Activity,
        "clock-1": Clock1, "clock-2": Clock2, "clock-3": Clock3,
        "clock-4": Clock4, "clock-5": Clock5, "clock-6": Clock6,
        "clock-7": Clock7, "clock-8": Clock8, "clock-9": Clock9,
        "clock-10": Clock10, "clock-11": Clock11, "clock-12": Clock12,
        "code": Code,
        "coffee": Coffee,
        "gamepad": Gamepad,
        "message-circle": MessageCircle,
        "mic": Mic, "mic-off": MicOff,
        "monitor": Monitor,
        "music": Music,
    } as const satisfies Record<string, Component>;

    export type IconName = keyof typeof ICONS;
</script>

<script lang="ts">
    type Props = {
        name: IconName;
        size?: number | string;
        class?: string;
    };

    let { name, ...rest }: Props = $props();
    let Component = $derived(ICONS[name]);
</script>

<Component {...rest} />
