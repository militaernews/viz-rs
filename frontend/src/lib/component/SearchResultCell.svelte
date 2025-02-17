<script lang="ts">
	import { formatDate } from '$lib/util';
	import type { SearchResult } from '../../routes/+page.server';

	const { result, onclick } = $props<{ result: SearchResult; onclick: () => void }>();
</script>

<button
	{onclick}
	class="bg-base-100 flex cursor-pointer flex-col items-center gap-2 overflow-clip rounded-lg border-2 text-sm text-gray-500 transition-transform hover:scale-96"
	class:border-green-500={result.similarity > 0.9}
	class:border-lime-500={result.similarity > 0.75 && result.similarity <= 0.9}
	class:border-yellow-500={result.similarity > 0.5 && result.similarity <= 0.75}
	class:border-amber-500={result.similarity > 0.35 && result.similarity <= 0.5}
	class:border-orange-500={result.similarity > 0.1 && result.similarity <= 0.35}
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
