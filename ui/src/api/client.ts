// Ported from Sowel ui/src/api/client.ts (Sowel commit 5da84dc1, UI 1.87.0). Cuts: the access token, its refresh on 401, `fetchPublic`. Added: a GET answered
// 503 is retried once after 1 s, as a 429 is (spec 014, R9.6).

export const API_BASE = "/api/v1";

/**
 * An HTTP failure with its status code, so a caller can tell a busy server (429) from a
 * missing record (404) or a dropped connection (0). The message is the server's own text,
 * exactly as the plain `Error` used to carry it, so callers that read `err.message` keep
 * working.
 *
 * Issue #1141 — `fromServer` is false when the client wrote the message itself (« Session
 * expired », `HTTP 403: Forbidden` when the body carried no `error`): that text is English
 * and not meant for the user, so `apiErrorMessage` translates by status instead.
 */
export class ApiError extends Error {
  readonly status: number;
  readonly fromServer: boolean;

  constructor(
    status: number,
    message: string,
    options?: { cause?: unknown; fromServer?: boolean },
  ) {
    super(message, options);
    this.name = "ApiError";
    this.status = status;
    this.fromServer = options?.fromServer ?? true;
  }
}

/** `fetch` itself rejects when the request never got an answer (offline, DNS, reset). */
async function send(url: string, init: RequestInit): Promise<Response> {
  try {
    return await fetch(url, init);
  } catch (err) {
    // An abort is the caller's own decision, not a network failure.
    if (err instanceof Error && err.name === "AbortError") throw err;
    throw new ApiError(0, err instanceof Error ? err.message : String(err), { cause: err });
  }
}

async function failure(response: Response): Promise<ApiError> {
  const body = await response.json().catch(() => ({}));
  const serverText = (body as { error?: unknown }).error;
  if (typeof serverText === "string" && serverText) {
    return new ApiError(response.status, serverText);
  }
  return new ApiError(response.status, `HTTP ${response.status}: ${response.statusText}`, {
    fromServer: false,
  });
}

/** A burst can 429 a legitimate read, and the box answers 503 when its workers are all busy. */
const RATE_LIMIT_MAX_WAIT_MS = 5000;

function parseRetryAfter(header: string | null): number {
  const seconds = Number(header);
  if (!Number.isFinite(seconds) || seconds <= 0) return 1000;
  return Math.min(seconds * 1000, RATE_LIMIT_MAX_WAIT_MS);
}

export async function fetchJSON<T>(
  url: string,
  options?: RequestInit,
  retried = false,
): Promise<T> {
  const headers: Record<string, string> = {};
  if (options?.body) {
    headers["Content-Type"] = "application/json";
  }

  const response = await send(url, {
    headers,
    ...options,
  });

  // Rate limited (429) or busy (503): retry reads once after the advertised delay (1 s when none).
  // Only GETs are retried — replaying a mutation blindly could apply it twice.
  const method = (options?.method ?? "GET").toUpperCase();
  if ((response.status === 429 || response.status === 503) && !retried && method === "GET") {
    await new Promise((resolve) =>
      setTimeout(resolve, parseRetryAfter(response.headers.get("retry-after"))),
    );
    return fetchJSON<T>(url, options, true);
  }

  if (!response.ok) throw await failure(response);

  if (response.status === 204) return undefined as T;
  return response.json() as Promise<T>;
}
