<script lang="ts">
	import '../app.css';
	let { children } = $props();
</script>

<div class=" container mx-auto max-w-3xl">
	{@render children()}
</div>
