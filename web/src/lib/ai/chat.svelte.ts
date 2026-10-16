import { apiFetch } from '$lib/api/client';
import { readEventStream } from '$lib/notifications/sse';

export interface ProposedAction {
	plugin: string;
	tool: string;
	function: string;
	arguments: Record<string, unknown>;
}

export interface ChatTurn {
	role: 'user' | 'assistant';
	content: string;
	/** Tools the assistant used to answer. */
	steps?: { tool: string; ok: boolean }[];
	/** Changes the assistant proposes; nothing happens until the person confirms. */
	proposals?: ProposedAction[];
	/** The answer is still arriving. */
	streaming?: boolean;
}

const CHAT = '/api/ui/ai/chat';

/** What the person reads for each refusal the server can give; anything else shows the server's own words. */
function describe(status: number, message: string): string {
	if (status === 403 && /turned off/i.test(message)) {
		return 'The assistant is turned off for this organization. An administrator can turn it on in Settings → AI assistant.';
	}
	if (status === 401) return 'Sign in again to use the assistant.';
	if (status === 429) return message;
	return message || `The assistant could not answer (${status}).`;
}

/**
 * A conversation with the assistant (`POST /api/ui/ai/chat`, streamed). The server runs plugins' tools as the
 * signed-in person; a change the assistant wants to make comes back as a proposal, which only happens when
 * `confirm` is called for it.
 */
export class AiChat {
	turns = $state<ChatTurn[]>([]);
	busy = $state(false);
	/** A problem with the last question (shown beside the input, not as a turn). */
	problem = $state<string | null>(null);
	private abort: AbortController | null = null;

	async send(text: string, context?: string): Promise<void> {
		const question = text.trim();
		if (!question || this.busy) return;
		this.problem = null;
		const history = this.turns.map(({ role, content }) => ({ role, content }));
		this.turns.push({ role: 'user', content: question });
		this.turns.push({ role: 'assistant', content: '', streaming: true, steps: [], proposals: [] });
		const answer = this.turns[this.turns.length - 1];
		this.busy = true;
		const abort = new AbortController();
		this.abort = abort;
		try {
			const response = await apiFetch(CHAT, {
				method: 'POST',
				headers: { 'content-type': 'application/json', accept: 'text/event-stream' },
				body: JSON.stringify({
					messages: [...history, { role: 'user', content: question }],
					context,
					stream: true
				}),
				signal: abort.signal
			});
			if (!response.ok || !response.body) {
				const body = (await response.json().catch(() => ({}))) as { error?: string };
				throw new Error(describe(response.status, body.error ?? ''));
			}
			await readEventStream(response.body, ({ event, data }) => {
				const payload = data ? (JSON.parse(data) as Record<string, any>) : {};
				if (event === 'delta') answer.content += String(payload.text ?? '');
				else if (event === 'step')
					answer.steps = [
						...(answer.steps ?? []),
						{ tool: String(payload.tool), ok: !!payload.ok }
					];
				else if (event === 'message') {
					// The whole answer replaces any words streamed before a tool was used.
					answer.content = String(payload.text ?? '');
					answer.proposals = (payload.proposed_actions ?? []) as ProposedAction[];
				} else if (event === 'error')
					throw new Error(String(payload.error ?? 'The assistant failed.'));
			});
		} catch (error) {
			if (abort.signal.aborted) return;
			this.problem = error instanceof Error ? error.message : 'The assistant could not answer.';
			// An empty answer is not a turn.
			if (!answer.content) this.turns.splice(this.turns.length - 2, 2);
		} finally {
			answer.streaming = false;
			this.busy = false;
			this.abort = null;
		}
	}

	/** Stop waiting for the answer. */
	stop(): void {
		this.abort?.abort();
	}

	clear(): void {
		this.stop();
		this.turns = [];
		this.problem = null;
	}

	/** Run a proposed change as the person: the plugin function is called with their own session. */
	async confirm(turn: ChatTurn, proposal: ProposedAction): Promise<string | null> {
		const response = await apiFetch(
			`/api/plugins/${encodeURIComponent(proposal.plugin)}/${encodeURIComponent(proposal.function)}`,
			{
				method: 'POST',
				headers: { 'content-type': 'application/json' },
				body: JSON.stringify(proposal.arguments ?? {})
			}
		);
		if (!response.ok) {
			const body = (await response.json().catch(() => ({}))) as { error?: string };
			return body.error ?? `It could not be done (${response.status}).`;
		}
		turn.proposals = (turn.proposals ?? []).filter((p) => p !== proposal);
		this.turns.push({ role: 'assistant', content: `Done: ${proposal.tool.replaceAll('_', ' ')}.` });
		return null;
	}

	/** The person decided against a proposal. */
	dismiss(turn: ChatTurn, proposal: ProposedAction): void {
		turn.proposals = (turn.proposals ?? []).filter((p) => p !== proposal);
	}
}
