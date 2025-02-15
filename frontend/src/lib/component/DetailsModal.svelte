<script lang="ts">
	import FluentColorCalendar24 from '~icons/fluent-color/calendar-24';
	import FluentColorChatMore24 from '~icons/fluent-color/chat-more-24';
	import FluentColorSearchVisual24 from '~icons/fluent-color/search-visual-24';
	import FluentColorMegaphoneLoud24 from '~icons/fluent-color/megaphone-loud-24';
	import FluentColorLinkMultiple24 from '~icons/fluent-color/link-multiple-24';
	import FluentColorCloudWords24 from '~icons/fluent-color/cloud-words-24';

	import type { SearchResult } from '../../routes/SearchResult';

	let {
		dialog = $bindable(),
		details = null
	}: {
		dialog: HTMLDialogElement | undefined;
		details: SearchResult | null;
	} = $props();
</script>

<dialog bind:this={dialog} class="modal modal-bottom">
	<div class="modal-box mx-auto flex max-w-3xl flex-row gap-4">
		<img
			src="{`/img/${details?.chat_id}/${details?.msg_id}`}.jpg"
			class="max-h-90 max-w-90 rounded-lg object-cover"
			alt="Image {details?.chat_id}/{details?.msg_id}"
			onerror={(ev) => {
				ev.target!.onerror = null;
				ev.target!.src = '/test.jpg';
				console.log(ev.target);
			}}
		/>

		<div class="flex w-full flex-col gap-2">
			<h3 class="text-lg font-bold">Name of channel</h3>

			<div class="grid w-full grid-cols-2 justify-between gap-2">
				<span class="flex flex-row items-center gap-1"
					><FluentColorMegaphoneLoud24 class="size-5" />Chat ID</span
				>
				<p class="">{details?.chat_id}</p>

				<span class="flex flex-row items-center gap-1"
					><FluentColorSearchVisual24 class="size-5" />Similarity</span
				>
				<p class="">{(details?.similarity! * 100).toFixed(2)}%</p>

				<span class="flex flex-row items-center gap-1"
					><FluentColorCalendar24 class="size-5" />Posted at</span
				>
				<p class=" ">
					{new Date(details?.posted_at!).toLocaleTimeString([], {
						year: 'numeric',
						month: 'numeric',
						day: 'numeric',
						hour: '2-digit',
						minute: '2-digit',
						hour12: false
					})}
				</p>
			</div>

			<div class="mt-auto flex w-full flex-row space-x-2">
				<a class="btn btn-accent flex-1" href=""><FluentColorChatMore24 class="size-5" /> Source</a>
				<a class="btn btn-primary flex-1" href=""
					><FluentColorLinkMultiple24 class="size-5" /> Invite</a
				>
				<a class="btn btn-secondary flex-1" href=""
					><FluentColorCloudWords24 class="size-5" /> Backup</a
				>
			</div>
		</div>
	</div>
	<form method="dialog" class="modal-backdrop">
		<button>close</button>
	</form>
</dialog>
