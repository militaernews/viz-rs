<script lang="ts">
	import '../app.css';
	let { children } = $props();
</script>

<div class="container mx-auto max-w-3xl p-2">
	{@render children()}
</div>
