<script lang="ts">
	import DetailsModal from '../lib/component/DetailsModal.svelte';
	import type { PageProps } from './$types';
	import SearchResultCell from '$lib/component/SearchResultCell.svelte';
	import { enhance } from '$app/forms';
	import { browser } from '$app/environment';

	// Using proper Fluent icons
	import FluentCloudArrowUp24Regular from '~icons/fluent/cloud-arrow-up-24-regular';
	import FluentTag24Regular from '~icons/fluent/tag-24-regular';
	import FluentCheckmarkCircle24Filled from '~icons/fluent/checkmark-circle-24-filled';
	import FluentDismissCircle24Filled from '~icons/fluent/dismiss-circle-24-filled';
	import FluentArrowClockwise24Regular from '~icons/fluent/arrow-clockwise-24-regular';
	import FluentSearch24Regular from '~icons/fluent/search-24-regular';
	import FluentSave24Regular from '~icons/fluent/save-24-regular';
	import FluentHistory24Regular from '~icons/fluent/history-24-regular';
	import FluentImage24Regular from '~icons/fluent/image-24-regular';
	import FluentCalendar24Regular from '~icons/fluent/calendar-24-regular';
	import type { SearchResult } from '$lib/SearchResult';

	// Enhanced SearchResult interface with base64 image
	export interface SearchResult {
		msg_id: number;
		chat_id: number;
		posted_at: string;
		similarity: number;
		display_name: string;
		user_name: string | null;
		bias: string | undefined;
		invite_hash: string | null;
		tags: string[];
		img: string; // base64 encoded image
	}

	// History entry interface
	interface SearchHistoryEntry {
		id: string;
		timestamp: number;
		results: SearchResult[];
		searchParams: {
			tags: string[];
			startDate: string;
			endDate: string;
			imageFileName?: string;
		};
	}

	let { form }: PageProps = $props();
	let dragActive = $state(false);
	let isLoading = $state(false);
	let fileInput: HTMLInputElement;
	let tags = $state<string[]>([]);
	let tagInput = $state('');
	let tagInputElement: HTMLInputElement;

	// Date states
	let startDate = $state('');
	let endDate = $state('');

	// Tab state
	let activeTab = $state<'image' | 'tags'>('image');

	let dialog: HTMLDialogElement | undefined = $state();
	let details: SearchResult | null = $state(null);

	// Local storage key for history
	const HISTORY_STORAGE_KEY = 'search_history_v2';

	// Convert image to base64
	async function imageToBase64(imageUrl: string): Promise<string> {
		try {
			const response = await fetch(imageUrl);
			const blob = await response.blob();
			return new Promise((resolve, reject) => {
				const reader = new FileReader();
				reader.onload = () => resolve(reader.result as string);
				reader.onerror = reject;
				reader.readAsDataURL(blob);
			});
		} catch (error) {
			console.error('Failed to convert image to base64:', error);
			return '';
		}
	}

	// Save search results to history
	async function saveToHistory(results: SearchResult[], imageFileName?: string) {
		if (!browser || results.length === 0) return;

		try {
			// Convert images to base64 if they aren't already
			const resultsWithBase64 = await Promise.all(
				results.map(async (result) => ({
					...result,
					img: result.img.startsWith('data:') ? result.img : await imageToBase64(result.img)
				}))
			);

			const historyEntry: SearchHistoryEntry = {
				id: crypto.randomUUID(),
				timestamp: Date.now(),
				results: resultsWithBase64,
				searchParams: {
					tags: [...tags],
					startDate,
					endDate,
					imageFileName
				}
			};

			// Get existing history
			const existingHistory = JSON.parse(localStorage.getItem(HISTORY_STORAGE_KEY) || '[]');

			// Add new entry at the beginning
			const updatedHistory = [historyEntry, ...existingHistory];

			// Keep only last 100 entries
			const limitedHistory = updatedHistory.slice(0, 100);

			// Save to localStorage
			localStorage.setItem(HISTORY_STORAGE_KEY, JSON.stringify(limitedHistory));

			console.log('Search saved to history:', historyEntry.id);
		} catch (error) {
			console.error('Failed to save to history:', error);
		}
	}

	// Load search history
	function loadSearchHistory(): Promise<SearchHistoryEntry[]> {
		if (!browser) return Promise.resolve([]);

		try {
			const history = JSON.parse(localStorage.getItem(HISTORY_STORAGE_KEY) || '[]');
			return Promise.resolve(history);
		} catch (error) {
			console.error('Failed to load search history:', error);
			return Promise.resolve([]);
		}
	}

	// Export current results as JSON
	function exportResults() {
		if (!form?.data || form.data.length === 0) return;

		const exportData = {
			results: form.data,
			exportedAt: new Date().toISOString(),
			searchParams: {
				tags,
				startDate,
				endDate
			}
		};

		const blob = new Blob([JSON.stringify(exportData, null, 2)], { type: 'application/json' });
		const url = URL.createObjectURL(blob);
		const a = document.createElement('a');
		a.href = url;
		a.download = `search_results_${new Date().toISOString().split('T')[0]}.json`;
		document.body.appendChild(a);
		a.click();
		document.body.removeChild(a);
		URL.revokeObjectURL(url);
	}

	// Navigate to history page - with fallback to show history in console/alert
	async function goToHistory() {
		try {
			// Try to navigate to history page
			window.location.href = '/history';
		} catch (error) {
			// Fallback: show history in a simple way
			const history = await loadSearchHistory();
			if (history.length === 0) {
				alert('No search history found.');
				return;
			}

			// Create a simple history display
			const historyDisplay = history
				.map(
					(entry, index) =>
						`${index + 1}. ${new Date(entry.timestamp).toLocaleDateString()} - ${entry.results.length} results`
				)
				.join('\n');

			if (
				confirm(
					`Search History:\n\n${historyDisplay}\n\nWould you like to export this history as JSON?`
				)
			) {
				const blob = new Blob([JSON.stringify(history, null, 2)], { type: 'application/json' });
				const url = URL.createObjectURL(blob);
				const a = document.createElement('a');
				a.href = url;
				a.download = `search_history_${new Date().toISOString().split('T')[0]}.json`;
				document.body.appendChild(a);
				a.click();
				document.body.removeChild(a);
				URL.revokeObjectURL(url);
			}
		}
	}

	const showModal = (index: number) => {
		const results = form?.data || [];
		details = results[index];
		dialog?.showModal();
		return () => dialog?.close();
	};

	function handleFileChange(event: Event) {
		const input = event.target as HTMLInputElement;
		if (input.files?.[0] && fileInput.form) {
			fileInput.form.requestSubmit();
		}
	}

	function handleDrag(e: DragEvent) {
		e.preventDefault();
		e.stopPropagation();
		dragActive = e.type === 'dragenter' || e.type === 'dragover';
	}

	function handleDrop(e: DragEvent) {
		e.preventDefault();
		e.stopPropagation();
		dragActive = false;

		if (e.dataTransfer?.files?.[0]) {
			const dt = new DataTransfer();
			dt.items.add(e.dataTransfer.files[0]);
			fileInput.files = dt.files;
			fileInput.dispatchEvent(new Event('change', { bubbles: true }));
		}
	}

	function initiateUpload() {
		if (!isLoading) {
			fileInput.click();
		}
	}

	function addTag() {
		const trimmedTag = tagInput.trim();
		if (trimmedTag && !tags.includes(trimmedTag)) {
			tags = [...tags, trimmedTag];
			tagInput = '';
			setTimeout(() => tagInputElement?.focus(), 0);
		}
	}

	function removeTag(tagToRemove: string) {
		tags = tags.filter((tag) => tag !== tagToRemove);
		setTimeout(() => tagInputElement?.focus(), 0);
	}

	function handleTagKeydown(e: KeyboardEvent) {
		if (e.key === 'Enter' || e.key === ',') {
			e.preventDefault();
			addTag();
		} else if (e.key === 'Backspace' && tagInput === '' && tags.length > 0) {
			e.preventDefault();
			removeTag(tags[tags.length - 1]);
		}
	}

	function clearFilters() {
		tags = [];
		startDate = '';
		endDate = '';
		tagInput = '';
	}

	// Clear current image selection
	function clearImage() {
		if (fileInput) {
			fileInput.value = '';
		}
	}

	// Reactive status
	const uploadStatus = $derived(
		isLoading
			? 'loading'
			: form?.error
				? 'error'
				: form?.data && form.data.length > 0
					? 'success'
					: 'idle'
	);

	const hasFilters = $derived(tags.length > 0 || startDate || endDate);
	const hasImage = $derived(fileInput?.files?.[0]);
	const displayResults = $derived(form?.data || []);

	// Save to history when new results arrive
	$effect(() => {
		if (form?.data && form.data.length > 0 && !isLoading) {
			const imageFileName = fileInput?.files?.[0]?.name;
			saveToHistory(form.data as SearchResult[], imageFileName);
		}
	});
</script>

<!-- Hidden form inputs -->
<input type="hidden" name="tags" value={tags.join(',')} />
<input type="hidden" name="startDate" value={startDate} />
<input type="hidden" name="endDate" value={endDate} />

<!-- Main Container -->
<div class="bg-base-100 min-h-screen">
	<!-- Compact Header -->
	<div class="bg-base-100/95 border-base-200 sticky top-0 z-10 border-b backdrop-blur-sm">
		<div class="container mx-auto max-w-7xl px-6 py-3">
			<form
				method="POST"
				enctype="multipart/form-data"
				use:enhance={() => {
					isLoading = true;
					dragActive = false;
					return async ({ update }) => {
						await update();
						isLoading = false;
					};
				}}
			>
				<!-- Tabs and History Button -->
				<div class="mb-0 flex items-center gap-4">
					<div class="tabs tabs-boxed bg-base-200 p-1">
						<button
							type="button"
							class="tab gap-2 {activeTab === 'image' ? 'tab-active' : ''}"
							onclick={() => (activeTab = 'image')}
						>
							<FluentImage24Regular class="h-4 w-4" />
							Image
						</button>
						<button
							type="button"
							class="tab gap-2 {activeTab === 'tags' ? 'tab-active' : ''}"
							onclick={() => (activeTab = 'tags')}
						>
							<FluentTag24Regular class="h-4 w-4" />
							Tags
						</button>
					</div>

					<button type="button" onclick={goToHistory} class="btn btn-ghost btn-sm gap-2">
						<FluentHistory24Regular class="h-4 w-4" />
						History
					</button>
				</div>

				<!-- Main Layout with Connected Tab Content -->
				<div class="bg-base-50 border-base-200 rounded-lg border">
					<div class="grid grid-cols-1 gap-0 lg:grid-cols-4">
						<!-- Main Content Area (3/4 width) - Fixed Height -->
						<div class="flex min-h-[200px] items-center p-6 lg:col-span-3">
							{#if activeTab === 'image'}
								<!-- Compact Image Upload -->
								<div
									class="relative w-full cursor-pointer rounded-lg border-2 border-dashed transition-all duration-200 {dragActive
										? 'border-primary bg-primary/5'
										: uploadStatus === 'error'
											? 'border-error bg-error/5'
											: uploadStatus === 'success'
												? 'border-success bg-success/5'
												: 'border-base-300 hover:border-primary/50 hover:bg-base-100'}"
									onclick={initiateUpload}
									ondragenter={handleDrag}
									ondragover={handleDrag}
									ondragleave={handleDrag}
									ondrop={handleDrop}
								>
									<input
										bind:this={fileInput}
										type="file"
										name="image"
										accept="image/*"
										disabled={isLoading}
										onchange={handleFileChange}
										class="sr-only"
									/>

									<div class="px-6 py-8 text-center">
										<div class="flex items-center justify-center gap-4">
											<!-- Status Icon -->
											<div
												class="rounded-full p-3 {uploadStatus === 'loading'
													? 'bg-primary/10'
													: uploadStatus === 'error'
														? 'bg-error/10'
														: uploadStatus === 'success'
															? 'bg-success/10'
															: dragActive
																? 'bg-primary/10'
																: 'bg-base-200'}"
											>
												{#if isLoading}
													<FluentArrowClockwise24Regular
														class="text-primary h-6 w-6 animate-spin"
													/>
												{:else if uploadStatus === 'error'}
													<FluentDismissCircle24Filled class="text-error h-6 w-6" />
												{:else if uploadStatus === 'success'}
													<FluentCheckmarkCircle24Filled class="text-success h-6 w-6" />
												{:else}
													<FluentCloudArrowUp24Regular class="text-base-content/60 h-6 w-6" />
												{/if}
											</div>

											<!-- Status Text - Horizontal Layout -->
											<div class="text-left">
												{#if isLoading}
													<h3 class="text-primary font-semibold">Processing image...</h3>
												{:else if uploadStatus === 'error'}
													<h3 class="text-error font-semibold">Upload failed</h3>
													<p class="text-error/80 text-sm">{form?.error || 'Try again'}</p>
												{:else if uploadStatus === 'success'}
													<h3 class="text-success font-semibold">Search complete!</h3>
												{:else}
													<h3 class="text-base-content font-semibold">
														{dragActive ? 'Drop image here' : 'Upload image to search'}
													</h3>
													<p class="text-base-content/60 text-sm">
														{dragActive
															? 'Release to start'
															: 'Drag & drop or click • PNG, JPG, GIF up to 10MB'}
													</p>
												{/if}
											</div>

											{#if hasImage && !isLoading}
												<button
													type="button"
													onclick={(e) => {
														e.stopPropagation();
														clearImage();
													}}
													class="btn btn-ghost btn-sm"
												>
													Clear
												</button>
											{/if}
										</div>
									</div>
								</div>
							{:else}
								<!-- Compact Tags Input -->
								<div class="w-full space-y-3">
									<div
										class="border-base-300 bg-base-100 focus-within:border-primary flex min-h-[120px] flex-wrap items-start gap-2 rounded-lg border-2 p-3"
									>
										{#each tags as tag}
											<span
												class="bg-primary/10 text-primary inline-flex items-center gap-1 rounded-full px-2 py-1 text-xs font-medium"
											>
												{tag}
												<button
													type="button"
													onclick={() => removeTag(tag)}
													class="hover:bg-primary/20 rounded-full p-0.5 transition-colors"
												>
													<span class="text-xs">×</span>
												</button>
											</span>
										{/each}

										{#if tags.length === 0}
											<div class="w-full py-4 text-center">
												<FluentTag24Regular class="text-base-content/30 mx-auto mb-2 h-8 w-8" />
												<p class="text-base-content/60 text-sm">Add tags to search</p>
											</div>
										{/if}
									</div>

									<input
										bind:this={tagInputElement}
										bind:value={tagInput}
										onkeydown={handleTagKeydown}
										onblur={addTag}
										placeholder="Type tags and press Enter..."
										class="input input-bordered w-full"
									/>
								</div>
							{/if}
						</div>

						<!-- Sidebar: Filters + Actions (1/4 width) - Fixed Height -->
						<div
							class="bg-base-100 border-base-200 min-h-[200px] space-y-4 border-l p-6 lg:col-span-1"
						>
							<!-- Date Filter -->
							<div>
								<div class="mb-3 flex items-center gap-2">
									<FluentCalendar24Regular class="text-base-content/60 h-4 w-4" />
									<span class="text-base-content/80 text-sm font-medium">Date Range</span>
								</div>

								<div class="space-y-2">
									<input
										type="date"
										bind:value={startDate}
										placeholder="From"
										class="input input-bordered input-sm w-full"
									/>
									<input
										type="date"
										bind:value={endDate}
										placeholder="To"
										class="input input-bordered input-sm w-full"
									/>
								</div>
							</div>

							<!-- Action Buttons -->
							<div class="space-y-2">
								<button
									type="submit"
									disabled={isLoading}
									class="btn btn-primary btn-sm w-full gap-2"
								>
									<FluentSearch24Regular class="h-4 w-4" />
									Search
								</button>

								{#if hasFilters}
									<button type="button" onclick={clearFilters} class="btn btn-ghost btn-sm w-full">
										Clear Filters
									</button>
								{/if}
							</div>
						</div>
					</div>
				</div>
			</form>
		</div>
	</div>

	<!-- Results - 4 Column Grid -->
	{#if displayResults.length > 0}
		<div class="container mx-auto max-w-7xl px-6 py-6">
			<div class="grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4">
				{#each displayResults as result, index}
					<SearchResultCell
						{result}
						onclick={() => showModal(index)}
						class="hover:shadow-shadow-lg cursor-pointer transition-all duration-200 hover:scale-[1.02]"
					/>
				{/each}
			</div>
		</div>

		<DetailsModal {details} bind:dialog />
	{/if}
</div>
