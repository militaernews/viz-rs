<script lang="ts">
	import FluentCloudArrowUp24Regular from '~icons/fluent/cloud-arrow-up-24-regular';
	import FluentCheckmarkCircle24Filled from '~icons/fluent/checkmark-circle-24-filled';
	import FluentDismissCircle24Filled from '~icons/fluent/dismiss-circle-24-filled';
	import FluentArrowClockwise24Regular from '~icons/fluent/arrow-clockwise-24-regular';

	interface Props {
		fileInput: HTMLInputElement;
		isLoading: boolean;
		uploadStatus: string;
		dragActive: boolean;
		hasFile: boolean;
		error?: string;
		accept: string;
		uploadText: string;
		dragText: string;
		onInitiateUpload: () => void;
		onFileChange: (event: Event) => void;
		onDrag: (e: DragEvent) => void;
		onDrop: (e: DragEvent) => void;
		onClear: () => void;
	}

	let {
		fileInput = $bindable(),
		isLoading,
		uploadStatus,
		dragActive,
		hasFile,
		error,
		accept,
		uploadText,
		dragText,
		onInitiateUpload,
		onFileChange,
		onDrag,
		onDrop,
		onClear
	}: Props = $props();
</script>

<div
	class="relative w-full cursor-pointer rounded-lg border-2 border-dashed transition-all duration-200 {dragActive
		? 'border-primary bg-primary/5'
		: uploadStatus === 'error'
			? 'border-error bg-error/5'
			: uploadStatus === 'success'
				? 'border-success bg-success/5'
				: 'border-base-300 hover:border-primary/50 hover:bg-base-100'}"
	onclick={onInitiateUpload}
	ondragenter={onDrag}
	ondragover={onDrag}
	ondragleave={onDrag}
	ondrop={onDrop}
>
	<input
		bind:this={fileInput}
		type="file"
		name="image"
		{accept}
		disabled={isLoading}
		onchange={onFileChange}
		class="sr-only"
	/>

	<div class="px-4 py-6 text-center">
		<div class="flex items-center justify-center gap-3">
			<div
				class="rounded-full p-2 {uploadStatus === 'loading'
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
					<FluentArrowClockwise24Regular class="text-primary h-5 w-5 animate-spin" />
				{:else if uploadStatus === 'error'}
					<FluentDismissCircle24Filled class="text-error h-5 w-5" />
				{:else if uploadStatus === 'success'}
					<FluentCheckmarkCircle24Filled class="text-success h-5 w-5" />
				{:else}
					<FluentCloudArrowUp24Regular class="text-base-content/60 h-5 w-5" />
				{/if}
			</div>

			<div class="text-left">
				{#if isLoading}
					<h3 class="text-primary text-sm font-semibold">Processing...</h3>
				{:else if uploadStatus === 'error'}
					<h3 class="text-error text-sm font-semibold">Upload failed</h3>
					<p class="text-error/80 text-xs">{error || 'Try again'}</p>
				{:else if uploadStatus === 'success'}
					<h3 class="text-success text-sm font-semibold">Upload complete!</h3>
				{:else}
					<h3 class="text-base-content text-sm font-semibold">
						{dragActive ? dragText : uploadText}
					</h3>
					<p class="text-base-content/60 text-xs">
						{dragActive ? 'Release to start' : 'Drag & drop or click'}
					</p>
				{/if}
			</div>

			{#if hasFile && !isLoading}
				<button
					type="button"
					onclick={(e) => {
						e.stopPropagation();
						onClear();
					}}
					class="btn btn-ghost btn-xs"
				>
					Clear
				</button>
			{/if}
		</div>
	</div>
</div>
