<script lang="ts">
    import StreamRenderer, { type StreamRendererProps } from "@/video/StreamRenderer.svelte";
    import { streamStatus } from "@/streams.svelte";
    import { strings } from "@/events.svelte";
    import Marquee from "@/components/Marquee.svelte";
    import Grid from "@/components/Grid.svelte";
    import ClockWidget from "@/widgets/ClockWidget.svelte";
    import LiveModeWidget from "@/widgets/LiveModeWidget.svelte";
    import CaptureWidget from "@/widgets/CaptureWidget.svelte";
    import AboutWidget from "@/widgets/AboutWidget.svelte";
    import KpmMeter from "@/KpmMeter.svelte";

    const liveMode = $derived(strings.value.$liveMode ?? "-");
    const captureInfo = $derived(strings.value.$captureInfo ?? "");
    const appRendererProps: Partial<StreamRendererProps> = $derived.by(() => {
        if (liveMode === "code") {
            return {
                colorKey: ["#1d2129", "#282e3a"],
                colorKeyKnee: [0.02, 0.18],
            } as const;
        } else {
            return {};
        }
    });

    const youtubeMusicRendererProps: Partial<StreamRendererProps> = {
        colorKey: "#212121",
        colorKeyKnee: [0.02, 0.18],
        binarizationColor: "#f17b29",
    };
</script>

<!--
    Pure viewer shell.  Stream lifecycle is fully server-managed — the
    frontend just renders two well-known stream IDs and polls for
    availability to show/hide the YouTube Music island.
-->
<Grid rows="1fr 60px" gap="2" class="w-screen h-screen p-2">
    <!-- Everything other than the YouTube Music island -->
    <Grid columns="1fr 3fr 40px" gap="2">
        <!-- Side Column: User Info -->
        <div class="flex! w-full h-full flex-col gap-2">
            <div class="island px-2 py-1.5">
                <ClockWidget />
            </div>
            <div class="island px-2 py-1.5">
                <LiveModeWidget />
                <CaptureWidget />
            </div>
            <div class="island px-3 py-2 flex-1">
                <pre class="font-sans font-light whitespace-pre-wrap wrap-break-word">{strings.value.message ?? ""}</pre>
            </div>
            <div class="island px-2 py-1.5">
                <AboutWidget />
            </div>
        </div>
        <!-- Main Column: Marquee + Main Stream -->
        <Grid rows="auto 1fr" gap="2">
            <!-- Top Row: Marquee Banner -->
            <div class="island overflow-clip">
                {#if strings.value.marquee}
                    <Marquee text={strings.value.marquee} />
                {/if}
            </div>
            <div class="island flex-col flex-1">
                <div class="flex-1 rounded-md items-center justify-center">
                    <StreamRenderer streamId="main" {...appRendererProps} />
                </div>
            </div>
        </Grid>
        <!-- Side Column: Action Panel -->
        <div class="island p-2 flex! w-full h-full flex-col">
            <KpmMeter />
        </div>
    </Grid>
    <!-- Bottom Row: YouTube Music (conditionally rendered) -->
    <div class="island flex! items-center justify-center pt-1">
        {#if streamStatus.hasYouTubeMusic}
            <StreamRenderer streamId="youtube-music" {...youtubeMusicRendererProps} />
        {/if}
    </div>
</Grid>
