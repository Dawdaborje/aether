<script>
	// The application shell (theme layout, navigation) is applied by the parent
	// `(app)` layout; plugin pages render inside it.
	let { children } = $props();
</script>

{@render children()}
