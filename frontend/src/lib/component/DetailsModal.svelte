<script lang="ts">
	// Proper Fluent icons
	import FluentCalendar24Regular from '~icons/fluent/calendar-24-regular';
	import FluentChat24Regular from '~icons/fluent/chat-24-regular';
	import FluentEye24Regular from '~icons/fluent/eye-24-regular';
	import FluentMegaphone24Regular from '~icons/fluent/megaphone-24-regular';
	import FluentLink24Regular from '~icons/fluent/link-24-regular';
	import FluentCloudWords24Regular from '~icons/fluent/cloud-words-24-regular';
	import FluentTag24Regular from '~icons/fluent/tag-24-regular';
	import FluentDismiss24Regular from '~icons/fluent/dismiss-24-regular';
	import FluentOpen24Regular from '~icons/fluent/open-24-regular';

	import { formatDate } from '$lib/util';
	import type { SearchResult } from '../../routes/+page.server';

	interface Props {
		dialog?: HTMLDialogElement;
		details: SearchResult | null;
	}

	interface SimilarityInfo {
		color: string;
		bgColor: string;
		label: string;
	}

	let { dialog = $bindable(), details }: Props = $props();

	// Derived values using $derived rune
	const sourceLink: string = $derived(
		details?.invite_hash && !details?.user_name
			? `https://t.me/c/${details?.chat_id}/${details?.msg_id}`
			: details?.user_name
				? `https://t.me/${details.user_name}/${details.msg_id}`
				: ''
	);

	const inviteLink: string = $derived(
		details?.invite_hash ? `https://t.me/joinchat/${details.invite_hash}` : ''
	);

	const backupLink: string = $derived(
		details?.msg_id ? `https://t.me/nn_backup/${details.msg_id}` : ''
	);

	// Get similarity info using $derived
	const similarityInfo: SimilarityInfo | null = $derived(
		details ? getSimilarityInfo(details.similarity) : null
	);

	function getSimilarityInfo(similarity: number): SimilarityInfo {
		if (similarity > 0.9)
			return {
				color: 'text-success',
				bgColor: 'bg-success/10',
				label: 'Excellent Match'
			};
		if (similarity > 0.75)
			return {
				color: 'text-primary',
				bgColor: 'bg-primary/10',
				label: 'Very Good Match'
			};
		if (similarity > 0.5)
			return {
				color: 'text-warning',
				bgColor: 'bg-warning/10',
				label: 'Good Match'
			};
		if (similarity > 0.35)
			return {
				color: 'text-info',
				bgColor: 'bg-info/10',
				label: 'Fair Match'
			};
		if (similarity > 0.1)
			return {
				color: 'text-secondary',
				bgColor: 'bg-secondary/10',
				label: 'Poor Match'
			};
		return {
			color: 'text-error',
			bgColor: 'bg-error/10',
			label: 'Very Poor Match'
		};
	}

	function closeModal(): void {
		dialog?.close();
	}

	function handleImageError(event: Event): void {
		const target = event.target as HTMLImageElement;
		if (target) {
			target.onerror = null;
			target.src = '/placeholder.svg';
		}
	}

	function handleBackdropClick(event: Event): void {
		event.preventDefault();
		closeModal();
	}
</script>

<dialog bind:this={dialog} class="modal backdrop-blur-sm">
	<div class="modal-box bg-base-100 mx-4 max-w-5xl overflow-hidden p-0 shadow-2xl">
		<!-- Header -->
		<header class="border-base-300 bg-base-200/50 flex items-center justify-between border-b p-6">
			<div class="flex items-center gap-3">
				<div class="h-8 w-2 {similarityInfo?.bgColor || 'bg-base-300'} rounded-full"></div>
				<div>
					<h2 class="text-base-content text-xl font-bold">
						{details?.display_name || 'Image Details'}
						{#if details?.bias}
							<span class="text-base-content/60 font-normal">• {details.bias}</span>
						{/if}
					</h2>
					<p class="text-base-content/70 mt-1 text-sm">
						Message #{details?.msg_id} • Chat {details?.chat_id}
					</p>
				</div>
			</div>
			<button
				class="btn btn-sm btn-circle btn-ghost hover:bg-error/10 hover:text-error"
				onclick={closeModal}
				aria-label="Close modal"
				type="button"
			>
				<FluentDismiss24Regular class="h-5 w-5" />
			</button>
		</header>

		<!-- Content -->
		<div class="flex flex-col gap-6 p-6 lg:flex-row">
			<!-- Image Section -->
			<div class="lg:w-1/2">
				<div class="bg-base-200 relative overflow-hidden rounded-xl shadow-lg">
					<img
						src="/img/{details?.chat_id}/{details?.msg_id}.jpg"
						class="h-auto max-h-96 w-full object-contain"
						alt="Full resolution image from {details?.display_name || 'unknown source'}"
						onerror={handleImageError}
					/>

					<!-- Similarity Badge on Image -->
					{#if similarityInfo && details}
						<div class="absolute top-4 right-4">
							<div
								class="
								bg-base-100/90 border-base-300/50 rounded-lg border px-3 py-2 text-sm
								font-semibold shadow-lg backdrop-blur-md {similarityInfo.color}
							"
							>
								{(details.similarity * 100).toFixed(1)}% • {similarityInfo.label}
							</div>
						</div>
					{/if}
				</div>
			</div>

			<!-- Details Section -->
			<div class="space-y-6 lg:w-1/2">
				<!-- Metadata Grid -->
				<div class="bg-base-200/50 rounded-xl p-5">
					<h3 class="text-base-content mb-4 flex items-center gap-2 text-lg font-semibold">
						<FluentChat24Regular class="text-primary h-5 w-5" />
						Message Details
					</h3>

					<dl class="grid grid-cols-1 gap-4">
						<div class="border-base-300/50 flex items-center justify-between border-b py-2">
							<dt class="text-base-content/80 flex items-center gap-2 text-sm font-medium">
								<FluentMegaphone24Regular class="h-4 w-4" />
								Chat ID
							</dt>
							<dd class="text-base-content font-mono text-sm">
								{details?.chat_id || 'N/A'}
							</dd>
						</div>

						<div class="border-base-300/50 flex items-center justify-between border-b py-2">
							<dt class="text-base-content/80 flex items-center gap-2 text-sm font-medium">
								<FluentEye24Regular class="h-4 w-4" />
								Similarity
							</dt>
							<dd class="text-sm font-semibold {similarityInfo?.color || 'text-base-content'}">
								{details ? (details.similarity * 100).toFixed(2) : '0'}%
							</dd>
						</div>

						<div class="flex items-center justify-between py-2">
							<dt class="text-base-content/80 flex items-center gap-2 text-sm font-medium">
								<FluentCalendar24Regular class="h-4 w-4" />
								Posted
							</dt>
							<dd class="text-base-content text-sm">
								{details?.posted_at ? formatDate(details.posted_at) : 'Unknown'}
							</dd>
						</div>
					</dl>
				</div>

				<!-- Tags Section -->
				{#if details?.tags?.length}
					<div class="bg-base-200/50 rounded-xl p-5">
						<h3 class="text-base-content mb-3 flex items-center gap-2 text-lg font-semibold">
							<FluentTag24Regular class="text-primary h-5 w-5" />
							Tags
						</h3>
						<div class="flex flex-wrap gap-2">
							{#each details.tags as tag}
								<span
									class="
									bg-primary/10 text-primary border-primary/20 hover:bg-primary/20 rounded-full border
									px-3 py-1.5 text-xs font-medium
									transition-colors duration-200
								"
								>
									{tag}
								</span>
							{/each}
						</div>
					</div>
				{/if}

				<!-- Action Buttons -->
				<div class="space-y-3">
					<h3 class="text-base-content flex items-center gap-2 text-lg font-semibold">
						<FluentOpen24Regular class="text-primary h-5 w-5" />
						Quick Actions
					</h3>

					<div class="grid grid-cols-1 gap-3">
						{#if sourceLink}
							<a
								class="btn btn-primary flex items-center justify-center gap-2 transition-all duration-200 hover:shadow-lg"
								href={sourceLink}
								target="_blank"
								rel="noopener noreferrer"
							>
								<FluentChat24Regular class="h-5 w-5" />
								View Source Message
								<FluentOpen24Regular class="ml-auto h-4 w-4 opacity-60" />
							</a>
						{/if}

						{#if inviteLink && details?.invite_hash && !details?.user_name}
							<a
								class="btn btn-secondary flex items-center justify-center gap-2 transition-all duration-200 hover:shadow-lg"
								href={inviteLink}
								target="_blank"
								rel="noopener noreferrer"
							>
								<FluentLink24Regular class="h-5 w-5" />
								Join Chat
								<FluentOpen24Regular class="ml-auto h-4 w-4 opacity-60" />
							</a>
						{/if}

						{#if backupLink}
							<a
								class="btn btn-accent flex items-center justify-center gap-2 transition-all duration-200 hover:shadow-lg"
								href={backupLink}
								target="_blank"
								rel="noopener noreferrer"
							>
								<FluentCloudWords24Regular class="h-5 w-5" />
								View Backup
								<FluentOpen24Regular class="ml-auto h-4 w-4 opacity-60" />
							</a>
						{/if}
					</div>
				</div>
			</div>
		</div>
	</div>

	<!-- Backdrop -->
	<form method="dialog" class="modal-backdrop bg-black/50">
		<button aria-label="Close modal" onclick={handleBackdropClick} type="submit">close</button>
	</form>
</dialog>
