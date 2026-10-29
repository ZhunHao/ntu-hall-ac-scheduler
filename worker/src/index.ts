import { env } from "cloudflare:workers";

type PresenceState = "home" | "away";

interface PresenceEvent {
	state: PresenceState;
	at: string;
	source: string;
}

const LAST_KEY = "presence:last";
const HISTORY_KEY = "presence:history";
const HISTORY_LIMIT = 200;

export default {
	async fetch(request) {
		const url = new URL(request.url);
		if (url.pathname !== "/presence") {
			return json({ error: "not found" }, 404);
		}
		if (!(await isAuthorized(request))) {
			return json({ error: "unauthorized" }, 401);
		}

		if (request.method === "GET") {
			const [last, history] = await Promise.all([
				env.PRESENCE.get<PresenceEvent>(LAST_KEY, "json"),
				env.PRESENCE.get<PresenceEvent[]>(HISTORY_KEY, "json"),
			]);
			return json({ last, history: history ?? [] });
		}

		if (request.method === "POST") {
			let body: unknown;
			try {
				body = await request.json();
			} catch {
				return json({ error: "body must be JSON" }, 400);
			}
			const event = parseEvent(body);
			if (!event) {
				return json({ error: 'state must be "home" or "away"' }, 400);
			}

			const history = (await env.PRESENCE.get<PresenceEvent[]>(HISTORY_KEY, "json")) ?? [];
			history.unshift(event);
			await Promise.all([
				env.PRESENCE.put(LAST_KEY, JSON.stringify(event)),
				env.PRESENCE.put(HISTORY_KEY, JSON.stringify(history.slice(0, HISTORY_LIMIT))),
			]);
			return json({ ok: true, last: event });
		}

		return json({ error: "method not allowed" }, 405);
	},
} satisfies ExportedHandler;

function parseEvent(body: unknown): PresenceEvent | null {
	if (typeof body !== "object" || body === null) return null;
	const { state, source } = body as Record<string, unknown>;
	if (state !== "home" && state !== "away") return null;
	return {
		state,
		at: new Date().toISOString(),
		source: typeof source === "string" && source.length <= 64 ? source : "unknown",
	};
}

// Compare SHA-256 digests so the check takes the same time whatever the token length
async function isAuthorized(request: Request): Promise<boolean> {
	const header = request.headers.get("Authorization") ?? "";
	const presented = header.startsWith("Bearer ") ? header.slice(7) : "";
	if (!presented || !env.PRESENCE_TOKEN) return false;
	const encoder = new TextEncoder();
	const [a, b] = await Promise.all([
		crypto.subtle.digest("SHA-256", encoder.encode(presented)),
		crypto.subtle.digest("SHA-256", encoder.encode(env.PRESENCE_TOKEN)),
	]);
	return crypto.subtle.timingSafeEqual(a, b);
}

function json(data: unknown, status = 200): Response {
	return Response.json(data, { status, headers: { "Cache-Control": "no-store" } });
}
