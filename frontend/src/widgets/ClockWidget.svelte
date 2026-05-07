<script lang="ts" module>
    import Grid from "@/components/Grid.svelte";
    import ClockComponent from "./ClockComponent.svelte";
</script>

<Grid columns="1fr 1fr" gap="2">
    <ClockComponent timeZone="Asia/Shanghai" label="UTC+8 Beijing" />
    <ClockComponent timeZone="America/New_York" label="UTC-4 New York" variant="secondary" />
</Grid>
