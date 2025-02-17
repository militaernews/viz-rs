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

	function handleDrag(e: DragEvent) {
		e.preventDefault();
		e.stopPropagation();
		if (e.type === 'dragenter' || e.type === 'dragover') {
			dragActive = true;
		} else if (e.type === 'dragleave') {
			dragActive = false;
		}
	}

	function handleDrop(e: DragEvent) {
		e.preventDefault();
		e.stopPropagation();
		dragActive = false;

		if (e.dataTransfer?.files && e.dataTransfer.files[0]) {
			dragActive = true;
			const dt = new DataTransfer();
			dt.items.add(e.dataTransfer.files[0]);
			fileInput.files = dt.files;
			fileInput.dispatchEvent(new Event('change', { bubbles: true }));
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

<div class=" bg-base-100 sticky top-0 z-10 p-4">
	<form
		method="POST"
		enctype="multipart/form-data"
		use:enhance={() => {
			isLoading = true;
			dragActive = false;
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
			class="btn btn-ghost bg-base-100 w-full cursor-pointer rounded-lg border-2 border-dashed p-8 text-center transition-colors"
			class:border-gray-500={!dragActive}
			class:border-blue-500={dragActive}
			class:border-orange-500={error}
			class:text-orange-500={error}
			class:text-gray-300={!error}
			ondragenter={handleDrag}
			ondragover={handleDrag}
			ondragleave={handleDrag}
			ondrop={handleDrop}
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
				{:else if form?.data?.length > 0}
					{form?.success}
				{:else}
					Drop images here or click to upload
				{/if}
			</p>
		</button>
	</form>
</div>

{#if form?.data?.length > 0}
	<div class="divide-accent grid grid-cols-4 gap-4 pb-4">
		{#each form!.data as result, index}
			<SearchResultCell {result} onclick={() => showModal(index)} />
		{/each}
	</div>
	<DetailsModal {details} bind:dialog />
{/if}
