<script lang="ts">
	import FluentCalendar24Regular from '~icons/fluent/calendar-24-regular';

	interface Props {
		startDate: string;
		endDate: string;
	}

	let { startDate = $bindable(), endDate = $bindable() }: Props = $props();
</script>

<div>
	<div class="mb-2 flex items-center gap-2">
		<FluentCalendar24Regular class="text-base-content/60 h-3 w-3" />
		<span class="text-base-content/80 text-xs font-medium">Date Range</span>
	</div>
	<div class="space-y-1">
		<input type="date" bind:value={startDate} class="input input-bordered input-xs w-full" />
		<input type="date" bind:value={endDate} class="input input-bordered input-xs w-full" />
	</div>
</div>
