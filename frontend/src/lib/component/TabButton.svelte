<script lang="ts">
	interface Props {
		active: boolean;
		onclick: () => void;
		icon: any;
		title: string;
	}

	let { active, onclick, icon: Icon, title }: Props = $props();
</script>

<button
	type="button"
	class="p-2 transition-colors {active ? 'bg-primary text-primary-content' : 'hover:bg-base-200'}"
	{onclick}
	{title}
>
	<Icon class="mx-auto h-4 w-4" />
</button>
