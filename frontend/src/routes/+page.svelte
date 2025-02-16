<script lang="ts">
	import DetailsModal from '../lib/component/DetailsModal.svelte';
	import SuperDebug, { superForm, fileProxy } from 'sveltekit-superforms';
	import type { PageData } from './$types';
	import { formatDate } from '$lib/util';
	import type { SearchResult } from './+page.server';

	let { data } = $props();

	let fileInput: HTMLInputElement;
	let dragActive = $state(false);
	let results = $state<SearchResult[]>([]);

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
		if (e.dataTransfer?.files?.length) {
			handleFile(e.dataTransfer.files[0]);
		}
	}

	const { form, enhance, errors, message } = superForm(data.form);

	function handleFile(file: File) {
		if (!file.type.startsWith('image/')) {
			$message = 'Please select an image file';
			return;
		}
		$form.image = file;
	}

	let dialog: HTMLDialogElement | undefined = $state();
	let details: SearchResult | null = $state(null);

	const showModal = (index: number) => {
		details = results[index];
		dialog?.showModal();
		return () => {
			dialog?.close();
		};
	};
</script>

<SuperDebug data={$form} />

// src/routes/+page.svelte
<div class="container mx-auto max-w-3xl p-2">
	<form method="POST" enctype="multipart/form-data" use:enhance>
		<!-- Upload Area -->

		<button
			type="button"
			class="sticky top-2 z-10 mb-2 w-full cursor-pointer rounded-lg border-2 border-dashed bg-slate-700/75 p-4 text-center backdrop-blur-sm transition-colors"
			class:border-blue-500={dragActive}
			class:border-gray-300={!dragActive}
			ondragover={handleDragOver}
			ondragleave={handleDragLeave}
			ondrop={handleDrop}
			onclick={() => fileInput.click()}
		>
			<input
				class="hidden"
				type="file"
				name="image"
				accept="image/png, image/jpeg"
				bind:this={fileInput}
				oninput={(e) => ($form.image = e.currentTarget.files?.item(0)!)}
			/>

			<p>
				{#if $form.image}
					{$form.image.name}
				{:else}
					Drop an image here or click to upload
				{/if}
			</p>
		</button>

		{#if $errors.image}
			<div class="mt-1 text-sm text-red-500">
				{$errors.image}
			</div>
		{/if}
	</form>

	<!-- Error Display -->
	{#if $message}
		<div class="mb-4 rounded border border-red-400 bg-red-100 px-4 py-3 text-red-700">
			{$message}
		</div>
	{/if}

	<!-- Results Display -->
	{#if results.length > 0}
		<div class="divide-accent grid grid-cols-4 gap-4 p-2">
			{#each results as result, i}
				<button
					onclick={() => showModal(i)}
					class="flex flex-col items-center gap-2 overflow-clip rounded-lg border-2 bg-slate-800 text-sm text-gray-500 transition-transform hover:scale-104"
					class:border-green-500={result.similarity > 0.8}
					class:border-lime-500={result.similarity > 0.65 && result.similarity <= 0.8}
					class:border-yellow-500={result.similarity > 0.4 && result.similarity <= 0.65}
					class:border-amber-500={result.similarity > 0.25 && result.similarity <= 0.4}
					class:border-orange-500={result.similarity > 0.1 && result.similarity <= 0.25}
					class:border-red-500={result.similarity <= 0.1}
				>
					<img
						src={`/img/${result.chat_id}/${result.msg_id}.jpg`}
						class="h-26 w-full object-cover"
						alt="Image {result.chat_id}/{result.msg_id}"
						onerror={(ev) => {
							ev.target.onerror = null;
							ev.target.src = '/placeholder.svg';
						}}
					/>
					<div class="flex flex-row items-center justify-between gap-2 px-2">
						<p class="font-semibold">{result.display_name}</p>
						<p>#{result.msg_id}</p>
					</div>
					<p class="px-2 pb-2">
						{formatDate(result.posted_at)}
					</p>
				</button>
			{/each}
		</div>
		<DetailsModal {details} bind:dialog />
	{/if}
</div>
