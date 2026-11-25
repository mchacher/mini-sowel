// Ported from Sowel ui/src/api/client.ts (Sowel commit 5da84dc1, UI 1.87.0): the access token, its
// refresh on 401 (spec 029, R9.4) and `fetchPublic`. Added: a GET answered 503 is retried once after
// 1 s, as a 429 is (spec 014, R9.6); `fetchSession` (the token, no refresh: logout); a 403 with
// `setupRequired` calls the handler the auth store sets (the home was reset: first-start screen).

export const API_BASE = "/api/v1";

// Token state, set by the auth store.
let accessToken: string | null = null;
let onUnauthorized: (() => Promise<boolean>) | null = null;
let onSetupRequired: (() => void) | null = null;

export function setAccessToken(token: string | null): void {
  accessToken = token;
}

/** The handler returns true when the session was refreshed (the request is then repeated once). */
export function setOnUnauthorized(handler: () => Promise<boolean>): void {
  onUnauthorized = handler;
}

export function setOnSetupRequired(handler: () => void): void {
  onSetupRequired = handler;
}

/** The access token, for the live connection's subprotocol. */
export function getAccessToken(): string | null {
  return accessToken;
}

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

interface Mode {
  /** Send the access token. */
  token: boolean;
  /** Ask for a refresh on a 401 and repeat the request once. */
  refresh: boolean;
}

async function request<T>(
  url: string,
  options: RequestInit | undefined,
  mode: Mode,
  retried = false,
  repeated = false,
): Promise<T> {
  const headers: Record<string, string> = {};
  const used = mode.token ? accessToken : null;
  if (used) headers["Authorization"] = `Bearer ${used}`;
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
    return request<T>(url, options, mode, true, repeated);
  }

  if (response.status === 401 && mode.refresh && onUnauthorized && !repeated) {
    // Another request may have refreshed the token while this one was on its way.
    const success = accessToken !== used && accessToken !== null ? true : await onUnauthorized();
    if (success) return request<T>(url, options, mode, retried, true);
    throw new ApiError(401, "Session expired", { fromServer: false });
  }

  if (response.status === 403 && mode.token && onSetupRequired) {
    const body = await response
      .clone()
      .json()
      .catch(() => ({}));
    if ((body as { setupRequired?: unknown }).setupRequired === true) onSetupRequired();
  }

  if (!response.ok) throw await failure(response);

  if (response.status === 204) return undefined as T;
  return response.json() as Promise<T>;
}

/** An authenticated request: the token, refreshed on a 401. */
export function fetchJSON<T>(url: string, options?: RequestInit): Promise<T> {
  return request<T>(url, options, { token: true, refresh: true });
}

/** A request without a token (status, setup, login, refresh). */
export function fetchPublic<T>(url: string, options?: RequestInit): Promise<T> {
  return request<T>(url, options, { token: false, refresh: false });
}

/** The token, but no refresh: logout, which a refused session must not retry. */
export function fetchSession<T>(url: string, options?: RequestInit): Promise<T> {
  return request<T>(url, options, { token: true, refresh: false });
}
