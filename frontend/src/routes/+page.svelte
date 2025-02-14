<script lang="ts">
	import { onMount } from 'svelte';

	let fileInput: HTMLInputElement;
	let dragActive = false;
	let isLoading = false;
	let results: Array<{
		msg_id: number;
		chat_id: number;
		posted_at: string;
		similarity: number;
	}> = [];
	let error: string | null = null;

	async function handleUpload(files: FileList) {
		if (!files.length) return;

		isLoading = true;
		error = null;

		try {
			const formData = new FormData();
			Array.from(files).forEach((file, index) => {
				formData.append(`image`, file);
			});

			const response = await fetch('http://localhost:3000/search', {
				method: 'POST',
				body: formData
			});

			if (!response.ok) {
				throw new Error(`HTTP error! status: ${response.status}`);
			}

			results = await response.json();
		} catch (e) {
			error = e instanceof Error ? e.message : 'An error occurred during upload';
			results = [];
		} finally {
			isLoading = false;
		}
	}

	function handleDragOver(e: DragEvent) {
		e.preventDefault();
		dragActive = true;
	}

	function handleDragLeave() {
		dragActive = false;
	}

	function handleDrop(e: DragEvent) {
		e.preventDefault();
		dragActive = false;

		if (e.dataTransfer?.files) {
			handleUpload(e.dataTransfer.files);
		}
	}
</script>

<div class="container mx-auto max-w-2xl p-4">
	<h1 class="mb-4 text-2xl font-bold">Image Search</h1>

	<!-- Upload Area -->
	<div
		class="mb-4 cursor-pointer rounded-lg border-2 border-dashed p-8 text-center transition-colors"
		class:border-blue-500={dragActive}
		class:border-gray-300={!dragActive}
		on:dragover={handleDragOver}
		on:dragleave={handleDragLeave}
		on:drop={handleDrop}
		on:click={() => fileInput.click()}
	>
		<input
			type="file"
			bind:this={fileInput}
			on:change={(e) => handleUpload(e.currentTarget.files!)}
			accept="image/*"
			multiple
			class="file-input hidden"
		/>
		<p>
			{#if isLoading}
				Uploading...
			{:else}
				Drop images here or click to upload
			{/if}
		</p>
	</div>

	<!-- Error Display -->
	{#if error}
		<div class="mb-4 rounded border border-red-400 bg-red-100 px-4 py-3 text-red-700">
			{error}
		</div>
	{/if}

	<!-- Results Display -->
	{#if results.length > 0}
		<div class="space-y-4">
			<h2 class="text-xl font-semibold">Search Results</h2>
			<div class="divide-accent grid grid-cols-3 gap-4 p-4">
				{#each results as result}
					<div class="flex items-center justify-between">
						<div>
							<p class="text-sm text-gray-600">Message ID: {result.msg_id}</p>
							<p class="text-sm text-gray-600">Chat ID: {result.chat_id}</p>
							<p class="text-sm text-gray-600">
								Posted: {new Date(result.posted_at).toLocaleString()}
							</p>
							<p class="text-lg font-semibold text-gray-600">
								Similarity: {(result.similarity * 100).toFixed(2)}%
							</p>
						</div>
					</div>
				{/each}
			</div>
		</div>
	{/if}
</div>
