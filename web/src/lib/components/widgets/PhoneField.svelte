<script lang="ts">
	/**
	 * Country + number: pick a country for its dialling code, then type the rest.
	 * Stores the number in `value` and optional country id in `countryField` on the form.
	 */
	import { Input } from '$lib/components/ui/input';
	import { Field, FieldLabel, FieldContent, FieldDescription } from '$lib/components/ui/field';
	import { SearchPicker, type SearchPickerItem } from '$lib/components/ui/search-picker';
	import { getFormState, getPageState, keyOf, rowsOf } from '$lib/pages/pageContext.svelte';

	let {
		id,
		name,
		label,
		required = false,
		help = '',
		countryField = '',
		countriesSource = 'country.list_countries'
	}: {
		id: string;
		name: string;
		label: string;
		required?: boolean;
		help?: string;
		/** Form field that holds the country link id. */
		countryField?: string;
		countriesSource?: string;
	} = $props();

	const form = getFormState();
	const page = getPageState();
	let ownPhone = $state('');
	let ownCountry = $state('');
	let items = $state<SearchPickerItem[]>([]);
	let prefixes = $state<Record<string, string>>({});
	let loading = $state(false);

	const phone = {
		get current(): string {
			return form && name ? String(form.values[name] ?? '') : ownPhone;
		},
		set current(next: string) {
			if (form && name) form.values[name] = next;
			else ownPhone = next;
		}
	};

	const country = {
		get current(): string {
			if (countryField && form) return String(form.values[countryField] ?? '');
			return ownCountry;
		},
		set current(next: string) {
			if (countryField && form) form.values[countryField] = next;
			else ownCountry = next;
		}
	};

	function sameId(a: string, b: string): boolean {
		if (!a || !b) return false;
		return a === b || keyOf(a) === keyOf(b) || a === keyOf(b) || keyOf(a) === b;
	}

	function prefixOf(countryId: string): string {
		if (!countryId) return '';
		for (const [id, prefix] of Object.entries(prefixes)) {
			if (sameId(id, countryId)) return prefix;
		}
		return '';
	}

	const selectedPrefix = $derived(prefixOf(country.current));

	$effect(() => {
		if (!page || !countriesSource) {
			items = [];
			return;
		}
		let cancelled = false;
		loading = true;
		void page.call(countriesSource, { limit: 500 }).then((result) => {
			if (cancelled) return;
			loading = false;
			if (!result.ok) {
				items = [];
				prefixes = {};
				return;
			}
			const nextPrefixes: Record<string, string> = {};
			const nextItems = rowsOf(result.data)
				.map((row) => {
					const id = String(row.id ?? '');
					if (!id) return null;
					const nameLabel =
						(typeof row.name === 'string' && row.name.trim()) ||
						(typeof row.code === 'string' && row.code.trim()) ||
						'Country';
					const prefix =
						typeof row.phone_prefix === 'string' ? row.phone_prefix.trim() : '';
					if (prefix) nextPrefixes[id] = prefix;
					return {
						id,
						label: prefix ? `${nameLabel} (${prefix})` : nameLabel,
						sublabel: typeof row.code === 'string' ? row.code : undefined
					} satisfies SearchPickerItem;
				})
				.filter((item): item is SearchPickerItem => item !== null);
			items = nextItems;
			prefixes = nextPrefixes;
		});
		return () => {
			cancelled = true;
		};
	});

	function onCountry(next: string) {
		const previousPrefix = selectedPrefix;
		country.current = next || '';
		const prefix = prefixOf(next);
		if (!prefix) return;
		const current = phone.current.trim();
		// Prefill dialling code when empty, or replace a lone previous prefix.
		if (!current || (previousPrefix && current === previousPrefix)) {
			phone.current = prefix;
		} else if (!current.startsWith('+') && !current.startsWith(prefix)) {
			phone.current = `${prefix}${current.replace(/^0+/, '')}`;
		}
	}
</script>

<Field data-part="form.field" data-field={name}>
	<FieldLabel for={id} class="text-xs font-medium text-muted-foreground">
		{label}{#if required}<span class="text-destructive"> *</span>{/if}
	</FieldLabel>
	<FieldContent>
		<div class="flex flex-col gap-2 sm:flex-row sm:items-start">
			<div class="sm:w-56 sm:shrink-0">
				<SearchPicker
					{items}
					value={country.current || null}
					preferId={country.current || null}
					placeholder={loading ? 'Loading…' : 'Country code…'}
					searchPlaceholder="Search country or code…"
					entityLabel="countries"
					clearable={!required}
					onchange={onCountry}
				/>
			</div>
			<div class="min-w-0 flex-1">
				<Input
					{id}
					{required}
					type="tel"
					inputmode="tel"
					autocomplete="tel"
					placeholder={selectedPrefix ? `${selectedPrefix}…` : label}
					value={phone.current}
					oninput={(event) => (phone.current = event.currentTarget.value)}
				/>
			</div>
		</div>
		{#if help}
			<FieldDescription>{help}</FieldDescription>
		{:else}
			<FieldDescription>Pick a country for the dialling code, then the number.</FieldDescription>
		{/if}
	</FieldContent>
</Field>
