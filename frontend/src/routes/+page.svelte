<script lang="ts">
	import DetailsModal from '../lib/component/DetailsModal.svelte';
	import SuperDebug, { superForm, fileProxy, type FormResult } from 'sveltekit-superforms';
	import type { ActionData, PageData } from './$types';
	import { formatDate } from '$lib/util';
	import type { SearchResult } from './+page.server';
	import { valibotClient } from 'sveltekit-superforms/adapters';
	import { searchFormSchema } from './schema';

	let { data, form: form2 } = $props();

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

	const { form, enhance, errors, message, delayed } = superForm(data.form, {
		validators: valibotClient(searchFormSchema),
		onUpdate({ form, result }) {
			console.log('onUpdate called:', result);

			const action = result.data as FormResult<ActionData>;

			if (form.valid && action?.results) {
				console.log('Results received:', action.results.length);
				results = action.results;
			} else {
				console.log('No results received:', action);
			}
		}
	});

	function handleFile(file: File) {
		if (!file.type.startsWith('image/')) {
			$message = 'Please select an image file';
			return;
		}
		$form.image = file;
	}
</script>

<SuperDebug data={$form} />

<form method="POST" enctype="multipart/form-data" use:enhance>
	<input
		class="sticky top-2 z-10 mb-2 w-full cursor-pointer rounded-lg border-2 border-dashed bg-slate-700/75 p-4 text-center backdrop-blur-sm transition-colors"
		class:border-blue-500={dragActive}
		class:border-gray-300={!dragActive}
		type="file"
		name="image"
		accept="image/png, image/jpeg"
		bind:this={fileInput}
		oninput={(e) => ($form.image = e.currentTarget.files?.item(0) as File)}
	/>

	{#if $errors.image}
		<div class="mt-1 text-sm text-red-500">
			{$errors.image}
		</div>
	{/if}

	{#if $delayed}<span class="loading loading-ring loading-sm"></span>{/if}

	{#if $errors.image}<span>{$errors.image}</span>{/if}
	<button class="btn">Submit</button>
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
{/if}
{#if data}
	data: {JSON.stringify(data)}
{/if}
{#if results}
	res: {results}
{/if}
{#if form2}
	form2: {JSON.stringify(form2)}
{/if}
{#if form}
	form3: {JSON.stringify($form)}
{/if}
