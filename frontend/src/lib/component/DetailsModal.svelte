<script lang="ts">
	import FluentColorCalendar24 from '~icons/fluent-color/calendar-24';
	import FluentColorChatMore24 from '~icons/fluent-color/chat-more-24';
	import FluentColorSearchVisual24 from '~icons/fluent-color/search-visual-24';
	import FluentColorMegaphoneLoud24 from '~icons/fluent-color/megaphone-loud-24';
	import FluentColorLinkMultiple24 from '~icons/fluent-color/link-multiple-24';
	import FluentColorCloudWords24 from '~icons/fluent-color/cloud-words-24';

	import { formatDate } from '$lib/util';
	import type { SearchResult } from '../../routes/+page.server';

	let {
		dialog = $bindable(),
		details = null
	}: {
		dialog: HTMLDialogElement | undefined;
		details: SearchResult | null;
	} = $props();

	let source_link = $derived(
		details?.invite_hash && !details?.user_name
			? `https://t.me/c/${details?.chat_id}/${details?.msg_id}`
			: `https://t.me/${details?.user_name}/${details?.msg_id}`
	);
	let invite_link = $derived(`https://t.me/joinchat/${details?.invite_hash}`);
	let backup_link = $derived(`https://t.me/nn_backup/${details?.msg_id}`); // todo add
</script>

<dialog bind:this={dialog} class="modal">
	<div class="modal-box mx-auto flex max-w-3xl flex-row gap-4">
		<img
			src="{`/img/${details?.chat_id}/${details?.msg_id}`}.jpg"
			class="max-h-90 max-w-90 rounded-lg object-cover"
			alt="Image {details?.chat_id}/{details?.msg_id}"
			onerror={(ev) => {
				ev.target!.onerror = null;
				ev.target!.src = '/placeholder.svg';
				console.log(ev.target);
			}}
		/>

		<div class="flex w-full flex-col gap-4">
			<h3 class="text-lg font-bold">
				{details?.display_name}{details?.bias && ` • ${details?.bias}`}
			</h3>

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
					{formatDate(details?.posted_at!)}
				</p>
			</div>

			{#if details?.tags?.length! > 0}
				<div class="flex flex-row flex-wrap items-center gap-2">
					{#each details?.tags! as tag}
						<span class="badge badge-soft badge-primary">{tag}</span>
					{/each}
				</div>
			{/if}

			<div class="mt-auto flex w-full flex-row space-x-2">
				<a class="btn btn-accent flex-1" href={source_link}>
					<FluentColorChatMore24 class="size-5" /> Source
				</a>
				{#if details?.invite_hash && !details?.user_name}<a
						class="btn btn-primary flex-1"
						href={invite_link}
						><FluentColorLinkMultiple24 class="size-5" /> Invite
					</a>
				{/if}
				<a class="btn btn-secondary flex-1" href={backup_link}
					><FluentColorCloudWords24 class="size-5" /> Backup</a
				>
			</div>
		</div>
	</div>
	<form method="dialog" class="modal-backdrop">
		<button>close</button>
	</form>
</dialog>
