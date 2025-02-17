<script lang="ts">
	import DetailsModal from '../lib/component/DetailsModal.svelte';
	import type { PageProps } from './$types';
	import type { SearchResult } from './+page.server';
	import SearchResultCell from '$lib/component/SearchResultCell.svelte';
	import { enhance } from '$app/forms';

	let { form }: PageProps = $props();
	let dragActive = $state(false);
	let isLoading = $state(false);
	let results: Array<SearchResult> = $state([]);
	let error: string | null = $state(null);
	let fileInput: HTMLInputElement;

	function handleFileChange(event: Event) {
		const input = event.target as HTMLInputElement;
		if (input.files && input.files[0]) {
			// Only submit if the upload was initiated by button click
			if (fileInput.form) {
				fileInput.form.requestSubmit();
			}
		}
	}

	function initiateUpload() {
		fileInput.click();
	}

	let dialog: HTMLDialogElement | undefined = $state();
	let details: SearchResult | null = $state(null);

	const showModal = (index: number) => {
		details = form?.data[index];
		dialog?.showModal();
		return () => {
			dialog?.close();
		};
	};
</script>

<form
	method="POST"
	enctype="multipart/form-data"
	use:enhance={() => {
		isLoading = true;
		error = null;
		return async ({ update }) => {
			isLoading = false;
			await update();
			if (form?.error) {
				error = form.error;
			}
		};
	}}
>
	<button
		type="button"
		class="sticky top-2 z-10 mb-2 w-full cursor-pointer rounded-lg border-2 border-dashed bg-slate-700/75 p-4 text-center backdrop-blur-sm transition-colors"
		class:border-blue-500={dragActive}
		class:border-gray-300={!dragActive}
		class:border-orange-500={error}
		class:text-orange-500={error}
		onclick={initiateUpload}
	>
		<input
			class="hidden"
			bind:this={fileInput}
			type="file"
			name="image"
			accept="image/*"
			onchange={handleFileChange}
		/>
		<p>
			{#if isLoading}
				Uploading...
			{:else if error}
				Error: {error}
			{:else}
				Drop images here or click to upload
			{/if}
		</p>
	</button>
</form>

{#if form?.data?.length > 0}
	<div class="divide-accent grid grid-cols-4 gap-4 p-2">
		{#each form!.data as result, index}
			<SearchResultCell {result} onclick={() => showModal(index)} />
		{/each}
	</div>
	<DetailsModal {details} bind:dialog />
{/if}
