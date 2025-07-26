<script lang="ts">
	import SearchResultCell from '$lib/component/SearchResultCell.svelte';
	import DetailsModal from '$lib/component/DetailsModal.svelte';
	import { browser } from '$app/environment';

	// Icons
	import FluentArrowLeft24Regular from '~icons/fluent/arrow-left-24-regular';
	import FluentDelete24Regular from '~icons/fluent/delete-24-regular';
	import FluentSave24Regular from '~icons/fluent/save-24-regular';
	import FluentImage24Regular from '~icons/fluent/image-24-regular';
	import FluentTag24Regular from '~icons/fluent/tag-24-regular';
	import FluentCalendar24Regular from '~icons/fluent/calendar-24-regular';
	import FluentVideo24Regular from '~icons/fluent/video-24-regular';

	// Types
	import type { SearchResult } from '$lib/SearchResult';
	import type { SearchHistoryEntry } from '$lib/SearchHistoryEntry';

	// State
	let searchHistory = $state<SearchHistoryEntry[]>([]);
	let selectedEntry = $state<SearchHistoryEntry | null>(null);
	let dialog: HTMLDialogElement | undefined = $state();
	let details: SearchResult | null = $state(null);
	let isLoading = $state(true);

	// Local storage key
	const HISTORY_STORAGE_KEY = 'search_history_v2';

	// Load history from localStorage
	function loadHistory() {
		if (!browser) return;

		try {
			const stored = localStorage.getItem(HISTORY_STORAGE_KEY);
			if (stored) {
				const parsed = JSON.parse(stored);
				if (Array.isArray(parsed)) {
					searchHistory = parsed.sort((a, b) => b.timestamp - a.timestamp);
				}
			}
		} catch (error) {
			console.error('Failed to load history:', error);
			searchHistory = [];
		} finally {
			isLoading = false;
		}
	}

	// Delete a history entry
	function deleteHistoryEntry(entryId: string) {
		searchHistory = searchHistory.filter((entry) => entry.id !== entryId);
		if (selectedEntry?.id === entryId) {
			selectedEntry = null;
		}

		// Save back to localStorage
		try {
			localStorage.setItem(HISTORY_STORAGE_KEY, JSON.stringify(searchHistory));
		} catch (error) {
			console.error('Failed to save after deletion:', error);
		}
	}

	// Clear all history
	function clearAllHistory() {
		if (confirm('Are you sure you want to clear all search history? This cannot be undone.')) {
			searchHistory = [];
			selectedEntry = null;

			try {
				localStorage.removeItem(HISTORY_STORAGE_KEY);
			} catch (error) {
				console.error('Failed to clear localStorage:', error);
			}
		}
	}

	// Export history entry
	function exportHistoryEntry(entry: SearchHistoryEntry) {
		try {
			const exportData = {
				...entry,
				exportedAt: new Date().toISOString()
			};

			const blob = new Blob([JSON.stringify(exportData, null, 2)], { type: 'application/json' });
			const url = URL.createObjectURL(blob);
			const a = document.createElement('a');
			a.href = url;
			a.download = `search_history_${new Date(entry.timestamp).toISOString().split('T')[0]}.json`;
			document.body.appendChild(a);
			a.click();
			document.body.removeChild(a);
			URL.revokeObjectURL(url);
		} catch (error) {
			console.error('Failed to export:', error);
			alert('Failed to export search history.');
		}
	}

	// Format date for display
	function formatDate(timestamp: number): string {
		return new Date(timestamp).toLocaleString();
	}

	// Show modal with result details
	function showModal(result: SearchResult) {
		details = result;
		dialog?.showModal();
	}

	// Go back to main search
	function goBack() {
		window.location.href = '/';
	}

	// Load history on mount
	$effect(() => {
		loadHistory();
	});

	// Listen for storage events (updates from other tabs)
	$effect(() => {
		if (!browser) return;

		const handleStorageChange = (e: StorageEvent) => {
			if (e.key === HISTORY_STORAGE_KEY) {
				loadHistory();
			}
		};

		window.addEventListener('storage', handleStorageChange);

		return () => {
			window.removeEventListener('storage', handleStorageChange);
		};
	});
</script>

<div class="bg-base-100 min-h-screen">
	<!-- Header -->
	<div class="bg-base-100/95 border-base-200 sticky top-0 z-10 border-b backdrop-blur-sm">
		<div class="container mx-auto max-w-7xl px-4 py-4">
			<div class="flex items-center justify-between">
				<div class="flex items-center gap-4">
					<button onclick={goBack} class="btn btn-ghost gap-2">
						<FluentArrowLeft24Regular class="h-4 w-4" />
						Back to Search
					</button>
					<h1 class="text-base-content text-2xl font-bold">Search History</h1>
				</div>

				{#if searchHistory.length > 0}
					<button onclick={clearAllHistory} class="btn btn-error btn-outline gap-2">
						<FluentDelete24Regular class="h-4 w-4" />
						Clear All
					</button>
				{/if}
			</div>
		</div>
	</div>

	<div class="container mx-auto max-w-7xl px-4 py-8">
		{#if isLoading}
			<!-- Loading -->
			<div class="py-16 text-center">
				<span class="loading loading-spinner loading-lg text-primary"></span>
				<p class="text-base-content/60 mt-4">Loading search history...</p>
			</div>
		{:else if searchHistory.length === 0}
			<!-- Empty State -->
			<div class="py-16 text-center">
				<div class="text-base-content/60 mb-4 text-6xl">📜</div>
				<h2 class="text-base-content mb-2 text-xl font-semibold">No search history yet</h2>
				<p class="text-base-content/60 mb-6">
					Your visual search results will appear here once you start searching.
				</p>
				<button onclick={goBack} class="btn btn-primary">Start Searching</button>
			</div>
		{:else}
			<div class="grid grid-cols-1 gap-8 lg:grid-cols-3">
				<!-- History List -->
				<div class="lg:col-span-1">
					<h2 class="text-base-content mb-4 text-lg font-semibold">
						Search History ({searchHistory.length})
					</h2>

					<div class="space-y-3">
						{#each searchHistory as entry}
							<div
								class="card bg-base-200 hover:bg-base-300 cursor-pointer transition-all {selectedEntry?.id ===
								entry.id
									? 'ring-primary ring-2'
									: ''}"
								onclick={() => (selectedEntry = entry)}
							>
								<div class="card-body p-4">
									<div class="mb-2 flex items-center justify-between">
										<span class="text-base-content/60 text-sm">
											{formatDate(entry.timestamp)}
										</span>
										<div class="flex gap-1">
											<button
												onclick={(e) => {
													e.stopPropagation();
													exportHistoryEntry(entry);
												}}
												class="btn btn-ghost btn-xs"
												title="Export"
											>
												<FluentSave24Regular class="h-3 w-3" />
											</button>
											<button
												onclick={(e) => {
													e.stopPropagation();
													deleteHistoryEntry(entry.id);
												}}
												class="btn btn-ghost btn-xs text-error hover:text-error"
												title="Delete"
											>
												<FluentDelete24Regular class="h-3 w-3" />
											</button>
										</div>
									</div>

									<div class="space-y-2">
										<!-- Results count -->
										<div class="text-sm font-medium">
											{entry.results.length} result{entry.results.length === 1 ? '' : 's'}
										</div>

										<!-- Search parameters -->
										<div class="text-base-content/60 space-y-1 text-xs">
											{#if entry.searchParams.imageFileName}
												<div class="flex items-center gap-1">
													<FluentImage24Regular class="h-3 w-3" />
													{entry.searchParams.imageFileName}
												</div>
											{/if}

											{#if entry.searchParams.videoFileName}
												<div class="flex items-center gap-1">
													<FluentVideo24Regular class="h-3 w-3" />
													{entry.searchParams.videoFileName}
												</div>
											{/if}

											{#if entry.searchParams.tags.length > 0}
												<div class="flex items-center gap-1">
													<FluentTag24Regular class="h-3 w-3" />
													{entry.searchParams.tags.join(', ')}
												</div>
											{/if}

											{#if entry.searchParams.startDate || entry.searchParams.endDate}
												<div class="flex items-center gap-1">
													<FluentCalendar24Regular class="h-3 w-3" />
													{entry.searchParams.startDate || '...'} - {entry.searchParams.endDate ||
														'...'}
												</div>
											{/if}
										</div>
									</div>
								</div>
							</div>
						{/each}
					</div>
				</div>

				<!-- Results Display -->
				<div class="lg:col-span-2">
					{#if selectedEntry}
						<div class="mb-6">
							<h2 class="text-base-content mb-2 text-lg font-semibold">
								Search Results from {formatDate(selectedEntry.timestamp)}
							</h2>
							<p class="text-base-content/60">
								{selectedEntry.results.length} result{selectedEntry.results.length === 1 ? '' : 's'}
							</p>
						</div>

						<div class="grid grid-cols-1 gap-6 sm:grid-cols-2 xl:grid-cols-3">
							{#each selectedEntry.results as result}
								<div class="group">
									<SearchResultCell
										{result}
										onclick={() => showModal(result)}
										class="cursor-pointer transition-all duration-200 hover:scale-[1.02] hover:shadow-lg"
									/>
								</div>
							{/each}
						</div>
					{:else}
						<!-- No selection state -->
						<div class="py-16 text-center">
							<div class="text-base-content/60 mb-4 text-4xl">👈</div>
							<h3 class="text-base-content mb-2 text-lg font-semibold">
								Select a search from history
							</h3>
							<p class="text-base-content/60">Click on any search entry to view its results.</p>
						</div>
					{/if}
				</div>
			</div>
		{/if}
	</div>

	<!-- Details Modal -->
	<DetailsModal {details} bind:dialog />
</div>
