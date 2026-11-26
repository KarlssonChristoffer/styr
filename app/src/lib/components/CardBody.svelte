<!-- What a board card draws, by its id: shared by Overview and Home, so a card is the same wherever it is put. A widget an agent wrote (`widgets.rs`) is drawn by WidgetCard.
     Used by Home.svelte, Overview.svelte. -->
<script lang="ts">
  // What a board card draws, by its id: shared by Overview and Home, so a card is the same
  // wherever it is put. A widget an agent wrote (`widgets.rs`) is drawn by WidgetCard.
  import { isWidget } from "../overview.svelte";
  import ActivityMap from "./ActivityMap.svelte";
  import ClockCard from "./ClockCard.svelte";
  import CustomsCard from "./CustomsCard.svelte";
  import DayCard from "./DayCard.svelte";
  import DaylightCard from "./DaylightCard.svelte";
  import DevlogCard from "./DevlogCard.svelte";
  import HealthCard from "./HealthCard.svelte";
  import LimitCard from "./LimitCard.svelte";
  import LinesCard from "./LinesCard.svelte";
  import MachinesCard from "./MachinesCard.svelte";
    import NowCard from "./NowCard.svelte";
  import ProjectsCard from "./ProjectsCard.svelte";
  import PulseCard from "./PulseCard.svelte";
  import RecentCard from "./RecentCard.svelte";
  import UseCard from "./UseCard.svelte";
  import RecordsCard from "./RecordsCard.svelte";
  import StatsCard from "./StatsCard.svelte";
  import TimeCard from "./TimeCard.svelte";
  import TimelineCard from "./TimelineCard.svelte";
  import WidgetCard from "./WidgetCard.svelte";

  let { id, show = () => {} }: { id: string; show?: (date: string) => void } = $props();
</script>

{#if id === "map"}<ActivityMap />
{:else if id === "day"}<DayCard />
{:else if id === "devlog"}<DevlogCard {show} />
{:else if id === "health"}<HealthCard />
{:else if id === "now"}<NowCard />
{:else if id === "pulse"}<PulseCard />
{:else if id === "limit"}<LimitCard />
{:else if id === "projects"}<ProjectsCard />
{:else if id === "timeline"}<TimelineCard />
{:else if id === "machines"}<MachinesCard />
{:else if id === "recent"}<RecentCard />
{:else if id === "customs"}<CustomsCard />
{:else if id === "stats"}<StatsCard />
{:else if id === "clock"}<ClockCard />
{:else if id === "daylight"}<DaylightCard />
{:else if id === "time"}<TimeCard />
{:else if id === "lines"}<LinesCard />
{:else if id === "records"}<RecordsCard />
{:else if id === "uselog"}<UseCard />
{:else if isWidget(id)}<WidgetCard {id} />
{/if}
