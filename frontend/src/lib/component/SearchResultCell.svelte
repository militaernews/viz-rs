<script lang="ts">
	import { formatDate } from '$lib/util';
	import FluentPerson24Regular from '~icons/fluent/person-24-regular';
	import FluentCalendar24Regular from '~icons/fluent/calendar-24-regular';
	import FluentEye24Regular from '~icons/fluent/eye-24-regular';
	import type { SearchResult } from '$lib/SearchResult';

	interface Props {
		result: SearchResult;
		onclick: () => void;
		class?: string;
	}

	let { result, onclick, class: className = '' }: Props = $props();

	interface SimilarityInfo {
		color: string;
		label: string;
		bgColor: string;
		borderColor: string;
	}

	// Get similarity color and label using $derived
	const similarityInfo: SimilarityInfo = $derived(getSimilarityInfo(result.similarity));

	function getSimilarityInfo(similarity: number): SimilarityInfo {
		if (similarity > 0.9)
			return {
				color: 'success',
				label: 'Excellent',
				bgColor: 'bg-success/10',
				borderColor: 'border-success/30'
			};
		if (similarity > 0.75)
			return {
				color: 'primary',
				label: 'Very Good',
				bgColor: 'bg-primary/10',
				borderColor: 'border-primary/30'
			};
		if (similarity > 0.5)
			return {
				color: 'warning',
				label: 'Good',
				bgColor: 'bg-warning/10',
				borderColor: 'border-warning/30'
			};
		if (similarity > 0.35)
			return {
				color: 'info',
				label: 'Fair',
				bgColor: 'bg-info/10',
				borderColor: 'border-info/30'
			};
		if (similarity > 0.1)
			return {
				color: 'secondary',
				label: 'Poor',
				bgColor: 'bg-secondary/10',
				borderColor: 'border-secondary/30'
			};
		return {
			color: 'error',
			label: 'Very Poor',
			bgColor: 'bg-error/10',
			borderColor: 'border-error/30'
		};
	}

	function handleClick(): void {
		onclick();
	}

	function handleKeydown(event: KeyboardEvent): void {
		if (event.key === 'Enter' || event.key === ' ') {
			event.preventDefault();
			onclick();
		}
	}

	function handleImageError(event: Event): void {
		const target = event.target as HTMLImageElement;
		if (target) {
			target.onerror = null;
			target.src = '/placeholder.svg';
		}
	}
</script>

<article
	class="
		group bg-base-100 relative cursor-pointer overflow-hidden rounded-xl border
		shadow-sm transition-all duration-300 ease-out
		hover:-translate-y-1 hover:scale-[1.02] hover:shadow-lg
		{similarityInfo.borderColor} {className}
	"
	onclick={handleClick}
	role="button"
	tabindex="0"
	onkeydown={handleKeydown}
>
	<!-- Image Container -->
	<div class="relative aspect-[4/3] overflow-hidden">
		{@debug result}
		<img
			src="data:image/png;base64,{result.img}"
			class="h-full w-full object-cover transition-transform duration-500 group-hover:scale-110"
			alt="Search result from {result.display_name}"
			loading="lazy"
			onerror={handleImageError}
		/>

		<!-- Similarity Badge -->
		<div class="absolute top-3 right-3">
			<div
				class="
				rounded-full px-2 py-1 text-xs font-semibold backdrop-blur-sm
				text-{similarityInfo.color} {similarityInfo.bgColor} border {similarityInfo.borderColor}
			"
			>
				{Math.round(result.similarity * 100)}%
			</div>
		</div>

		<!-- Gradient Overlay -->
		<div
			class="absolute inset-x-0 bottom-0 h-1/3 bg-gradient-to-t from-black/60 to-transparent opacity-0 transition-opacity duration-300 group-hover:opacity-100"
		></div>
	</div>

	<!-- Content -->
	<div class="space-y-3 p-4">
		<!-- Header -->
		<div class="flex items-start justify-between gap-2">
			<h3
				class="text-base-content group-hover:text-primary line-clamp-2 font-semibold transition-colors duration-200"
			>
				{result.display_name}
			</h3>
			<span class="text-base-content/50 shrink-0 font-mono text-xs">
				#{result.msg_id}
			</span>
		</div>

		<!-- Metadata -->
		<div class="space-y-2">
			<div class="text-base-content/70 flex items-center gap-2 text-xs">
				<FluentCalendar24Regular class="h-3.5 w-3.5" />
				<time datetime={result.posted_at}>
					{formatDate(result.posted_at)}
				</time>
			</div>

			<div class="flex items-center justify-between text-xs">
				<span class="text-base-content/60 flex items-center gap-1.5">
					<FluentEye24Regular class="h-3.5 w-3.5" />
					Match: {similarityInfo.label}
				</span>
			</div>
		</div>
	</div>

	<!-- Hover Overlay -->
	<div
		class="bg-primary/5 pointer-events-none absolute inset-0 opacity-0 transition-opacity duration-300 group-hover:opacity-100"
	></div>
</article>
