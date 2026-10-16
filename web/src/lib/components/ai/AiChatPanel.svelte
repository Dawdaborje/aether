<script lang="ts">
	import { tick } from 'svelte';
	import { AiChat, type ChatTurn, type ProposedAction } from '$lib/ai/chat.svelte';
	import { Button } from '$lib/components/ui/button';
	import { Textarea } from '$lib/components/ui/textarea';

	let {
		context,
		placeholder = 'Ask about your leave, payslips, approvals…'
	}: {
		/** What the person is looking at, told to the assistant (a page title, a record). */
		context?: string;
		placeholder?: string;
	} = $props();

	const chat = new AiChat();
	let draft = $state('');
	let confirming = $state<string | null>(null);
	let confirmProblem = $state<string | null>(null);
	let scroller = $state<HTMLElement | null>(null);

	async function ask() {
		const text = draft;
		draft = '';
		const done = chat.send(text, context);
		await tick();
		scroller?.scrollTo({ top: scroller.scrollHeight });
		await done;
		await tick();
		scroller?.scrollTo({ top: scroller.scrollHeight });
	}

	function onKey(event: KeyboardEvent) {
		if (event.key === 'Enter' && !event.shiftKey && !event.isComposing) {
			event.preventDefault();
			void ask();
		}
	}

	function label(tool: string): string {
		// `hr_leave__leave_balance` → "leave balance"
		return (tool.split('__')[1] ?? tool).replaceAll('_', ' ');
	}

	async function confirm(turn: ChatTurn, proposal: ProposedAction) {
		confirming = proposal.tool;
		confirmProblem = await chat.confirm(turn, proposal);
		confirming = null;
	}
</script>

<section
	class="flex h-full min-h-[24rem] flex-col rounded-lg border border-border bg-card"
	data-part="ai.chat"
>
	<div bind:this={scroller} class="flex-1 space-y-3 overflow-y-auto p-4" aria-live="polite">
		{#if chat.turns.length === 0}
			<p class="text-sm text-muted-foreground">
				Ask a question and the assistant looks it up with your own access. It cannot change anything
				without you confirming it.
			</p>
		{/if}
		{#each chat.turns as turn, i (i)}
			<div class="flex {turn.role === 'user' ? 'justify-end' : 'justify-start'}">
				<div
					class="max-w-[85%] space-y-2 rounded-lg px-3 py-2 text-sm whitespace-pre-wrap {turn.role ===
					'user'
						? 'bg-primary text-primary-foreground'
						: 'bg-muted text-foreground'}"
				>
					{#if turn.steps?.length}
						<p class="text-xs text-muted-foreground">
							Looked up: {turn.steps
								.map((s) => label(s.tool) + (s.ok ? '' : ' (failed)'))
								.join(', ')}
						</p>
					{/if}
					{#if turn.content}
						<p>{turn.content}</p>
					{:else if turn.streaming}
						<p class="text-muted-foreground">Thinking…</p>
					{/if}
					{#each turn.proposals ?? [] as proposal (proposal.tool + JSON.stringify(proposal.arguments))}
						<div class="space-y-2 rounded-md border border-border bg-card p-2 text-foreground">
							<p class="font-medium">Proposed: {label(proposal.tool)}</p>
							<dl class="grid grid-cols-[auto_1fr] gap-x-3 text-xs text-muted-foreground">
								{#each Object.entries(proposal.arguments ?? {}) as [name, value] (name)}
									<dt>{name.replaceAll('_', ' ')}</dt>
									<dd class="text-foreground">
										{typeof value === 'object' ? JSON.stringify(value) : String(value)}
									</dd>
								{/each}
							</dl>
							<div class="flex gap-2">
								<Button
									size="sm"
									disabled={confirming !== null}
									onclick={() => confirm(turn, proposal)}
								>
									{confirming === proposal.tool ? 'Working…' : 'Confirm'}
								</Button>
								<Button size="sm" variant="outline" onclick={() => chat.dismiss(turn, proposal)}
									>Not now</Button
								>
							</div>
						</div>
					{/each}
				</div>
			</div>
		{/each}
	</div>
	{#if chat.problem || confirmProblem}
		<p class="border-t border-border px-4 py-2 text-sm text-destructive" role="alert">
			{chat.problem ?? confirmProblem}
		</p>
	{/if}
	<form
		class="flex items-end gap-2 border-t border-border p-3"
		onsubmit={(event) => {
			event.preventDefault();
			void ask();
		}}
	>
		<Textarea
			bind:value={draft}
			{placeholder}
			rows={2}
			class="min-h-10 flex-1"
			onkeydown={onKey}
			disabled={chat.busy}
			aria-label="Message the assistant"
		/>
		{#if chat.busy}
			<Button type="button" variant="outline" onclick={() => chat.stop()}>Stop</Button>
		{:else}
			<Button type="submit" disabled={!draft.trim()}>Send</Button>
		{/if}
		{#if chat.turns.length > 0 && !chat.busy}
			<Button type="button" variant="ghost" onclick={() => chat.clear()}>New chat</Button>
		{/if}
	</form>
</section>
