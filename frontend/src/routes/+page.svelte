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

<div class="container mx-auto max-w-3xl p-2">
	<!-- Upload Area -->
	<div
		class="sticky top-2 z-10 mb-2 cursor-pointer rounded-lg border-2 border-dashed bg-slate-700/75 p-4 text-center backdrop-blur-sm transition-colors"
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
		<div class="divide-accent grid grid-cols-4 gap-4 p-2">
			{#each results as result}
				<a
					class="flex flex-col items-center gap-2 overflow-clip rounded-lg bg-slate-800 text-sm text-gray-500 transition-transform hover:scale-104"
					href={`https://t.me/${result.chat_id}/${result.msg_id}`}
				>
					<img
						src="/GX9CzFCaUAE5JxT.jpg"
						class="h-26 w-full object-cover"
						alt="Image {result.chat_id}/{result.msg_id}"
					/>
					<div class="flex flex-row items-center justify-between px-2">
						<p class=" font-semibold">{result.chat_id}</p>

						<p>#{result.msg_id}</p>
					</div>

					<div class="flex flex-row items-center justify-between px-2">
						<p>{new Date(result.posted_at).toLocaleString()}</p>

						<p
							class=" font-semibold text-green-500"
							class:text-green-500={result.similarity > 0.8}
							class:text-lime-500={result.similarity > 0.65 && result.similarity <= 0.8}
							class:text-yellow-500={result.similarity > 0.4 && result.similarity <= 0.65}
							class:text-amber-500={result.similarity > 0.25 && result.similarity <= 0.4}
							class:text-orange-500={result.similarity > 0.1 && result.similarity <= 0.25}
							class:text-red-500={result.similarity <= 0.1}
						>
							{(result.similarity * 100).toFixed(2)}%
						</p>
					</div>
				</a>
			{/each}
		</div>
		<a class="btn btn-secondary" href="">Load more More</a>
	{/if}
</div>
