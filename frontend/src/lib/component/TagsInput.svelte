<script lang="ts">
	import FluentTag24Regular from '~icons/fluent/tag-24-regular';

	interface Props {
		tags: string[];
		tagInput: string;
		tagInputElement: HTMLInputElement;
		onAddTag: () => void;
		onRemoveTag: (tag: string) => void;
		onTagKeydown: (e: KeyboardEvent) => void;
	}

	let {
		tags,
		tagInput = $bindable(),
		tagInputElement = $bindable(),
		onAddTag,
		onRemoveTag,
		onTagKeydown
	}: Props = $props();
</script>

<div class="w-full space-y-2">
	<div
		class="border-base-300 bg-base-100 focus-within:border-primary flex min-h-[100px] flex-wrap items-start gap-2 rounded-lg border-2 p-3"
	>
		{#each tags as tag}
			<span
				class="bg-primary/10 text-primary inline-flex items-center gap-1 rounded-full px-2 py-1 text-xs font-medium"
			>
				{tag}
				<button
					type="button"
					onclick={() => onRemoveTag(tag)}
					class="hover:bg-primary/20 rounded-full p-0.5 transition-colors"
				>
					<span class="text-xs">×</span>
				</button>
			</span>
		{/each}

		{#if tags.length === 0}
			<div class="w-full py-2 text-center">
				<FluentTag24Regular class="text-base-content/30 mx-auto mb-1 h-6 w-6" />
				<p class="text-base-content/60 text-xs">Add tags to search</p>
			</div>
		{/if}
	</div>

	<input
		bind:this={tagInputElement}
		bind:value={tagInput}
		onkeydown={onTagKeydown}
		onblur={onAddTag}
		placeholder="Type tags and press Enter..."
		class="input input-bordered input-sm w-full"
	/>
</div>
