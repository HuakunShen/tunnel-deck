/**
 * `@ssh-tunnel/client` — typed `fetch` client for the daemon API. Host-free:
 * the caller injects `fetch` (browser, Node 18+, DSH, VS Code all qualify),
 * and every response is validated against the contract before it is
 * returned. Responses use `.loose()` so newer daemons stay compatible.
 */
import {
  healthResponseSchema,
  hostListResponseSchema,
  isTunnelProblem,
  launchResponseSchema,
  logTailResponseSchema,
  openUrlResponseSchema,
  previewResponseSchema,
  profileListResponseSchema,
  stateListResponseSchema,
  terminalListResponseSchema,
  tunnelProfileSchema,
  tunnelStateSchema,
  TunnelError,
  type LaunchMode,
  type LogTail,
  type SshHostInfo,
  type TerminalTarget,
  type TunnelProblemCode,
  type TunnelProfile,
  type TunnelProfileInput,
  type TunnelState,
  launchPath,
  logPath,
  previewPath,
  probePath,
  profilePath,
  startPath,
  statePath,
  stopPath,
} from "@ssh-tunnel/contract";

export interface TunnelClientOptions {
  readonly baseUrl: string;
  readonly fetch: typeof fetch;
}

function problemToError(problem: unknown): TunnelError {
  if (isTunnelProblem(problem)) {
    return new TunnelError(problem.code, problem.message, {
      ...(problem.details === undefined ? {} : { details: problem.details }),
      retryable: problem.retryable,
    });
  }
  return new TunnelError("InternalError", "daemon returned an unknown error shape");
}

async function request<T>(
  options: TunnelClientOptions,
  method: string,
  path: string,
  parse: (body: unknown) => T,
  body?: unknown,
): Promise<T> {
  let response: Response;
  try {
    response = await options.fetch(`${options.baseUrl}${path}`, {
      method,
      headers: body === undefined ? {} : { "content-type": "application/json" },
      body: body === undefined ? undefined : JSON.stringify(body),
    });
  } catch (error) {
    throw new TunnelError(
      "InternalError",
      `cannot reach daemon at ${options.baseUrl}: ${error instanceof Error ? error.message : "fetch failed"}`,
      { retryable: true },
    );
  }
  let payload: unknown = null;
  try {
    payload = (await response.json()) as unknown;
  } catch {
    payload = null;
  }
  if (!response.ok) {
    const problem =
      typeof payload === "object" && payload !== null && "problem" in payload
        ? (payload as { problem: unknown }).problem
        : null;
    throw problemToError(problem);
  }
  try {
    return parse(payload);
  } catch {
    const code: TunnelProblemCode = "InternalError";
    throw new TunnelError(code, "daemon response failed validation");
  }
}

function must<T>(parsed: { success: boolean; data?: T }): T {
  if (!parsed.success || parsed.data === undefined) {
    throw new TunnelError("InternalError", "daemon response failed validation");
  }
  return parsed.data;
}

/** Create the typed client. All methods throw `TunnelError` on failure. */
export function createTunnelClient(options: TunnelClientOptions): {
  health(): Promise<{ ok: true; version: string; serviceInstanceId: string }>;
  listProfiles(): Promise<readonly TunnelProfile[]>;
  createProfile(input: TunnelProfileInput): Promise<TunnelProfile>;
  getProfile(id: string): Promise<TunnelProfile>;
  updateProfile(id: string, input: TunnelProfileInput): Promise<TunnelProfile>;
  deleteProfile(id: string): Promise<void>;
  getLog(id: string, tail?: number): Promise<LogTail>;
  preview(id: string): Promise<{ profileId: string; argv: string[]; command: string }>;
  listStates(): Promise<readonly TunnelState[]>;
  getState(id: string): Promise<TunnelState>;
  start(id: string): Promise<TunnelState>;
  stop(id: string): Promise<TunnelState>;
  stopAll(): Promise<readonly TunnelState[]>;
  probe(id: string): Promise<TunnelState>;
  listHosts(): Promise<readonly SshHostInfo[]>;
  listTerminals(): Promise<readonly TerminalTarget[]>;
  launchInTerminal(
    id: string,
    terminalId: string,
    mode?: LaunchMode,
  ): Promise<{
    profileId: string;
    terminalId: string;
    mode: LaunchMode;
    command: string;
    pid: number;
    launchedAt: string;
  }>;
  /** Hand a URL to the OS default handler (the user's own browser). */
  openUrl(url: string): Promise<{ url: string; openedAt: string }>;
  closeSession(id: string): Promise<void>;
} {
  return {
    health: () =>
      request(options, "GET", "/api/health", (body) =>
        must(healthResponseSchema.loose().safeParse(body)),
      ),
    listProfiles: () =>
      request(options, "GET", "/api/profiles", (body) =>
        must(profileListResponseSchema.loose().safeParse(body)).profiles,
      ),
    createProfile: (input) =>
      request(options, "POST", "/api/profiles", (body) =>
        must(tunnelProfileSchema.loose().safeParse(body)), input),
    getProfile: (id) =>
      request(options, "GET", profilePath(id), (body) =>
        must(tunnelProfileSchema.loose().safeParse(body))),
    updateProfile: (id, input) =>
      request(options, "PUT", profilePath(id), (body) =>
        must(tunnelProfileSchema.loose().safeParse(body)), input),
    deleteProfile: async (id): Promise<void> => {
      await request(options, "DELETE", profilePath(id), () => undefined);
    },
    preview: (id) =>
      request(options, "GET", previewPath(id), (body) =>
        must(previewResponseSchema.loose().safeParse(body))),
    listStates: () =>
      request(options, "GET", "/api/states", (body) =>
        must(stateListResponseSchema.loose().safeParse(body)).states,
      ),
    getState: (id) =>
      request(options, "GET", statePath(id), (body) =>
        must(tunnelStateSchema.loose().safeParse(body))),
    start: (id) =>
      request(options, "POST", startPath(id), (body) =>
        must(tunnelStateSchema.loose().safeParse(body))),
    stop: (id) =>
      request(options, "POST", stopPath(id), (body) =>
        must(tunnelStateSchema.loose().safeParse(body))),
    stopAll: () =>
      request(options, "POST", "/api/states/stop-all", (body) =>
        must(stateListResponseSchema.loose().safeParse(body)).states,
      ),
    probe: (id) =>
      request(options, "POST", probePath(id), (body) =>
        must(tunnelStateSchema.loose().safeParse(body))),
    listHosts: () =>
      request(options, "GET", "/api/ssh-hosts", (body) =>
        must(hostListResponseSchema.loose().safeParse(body)).hosts,
      ),
    listTerminals: () =>
      request(options, "GET", "/api/terminals", (body) =>
        must(terminalListResponseSchema.loose().safeParse(body)).terminals,
      ),
    getLog: (id, tail) =>
      request(options, "GET", logPath(id, tail), (body) =>
        must(logTailResponseSchema.loose().safeParse(body)),
      ),
    launchInTerminal: (id, terminalId, mode) =>
      request(options, "POST", launchPath(id), (body) =>
        must(launchResponseSchema.loose().safeParse(body)), { terminalId, mode }),
    openUrl: (url) =>
      request(options, "POST", "/api/open-url", (body) =>
        must(openUrlResponseSchema.loose().safeParse(body)), { url }),
    closeSession: async (id): Promise<void> => {
      await request(options, "DELETE", `${profilePath(id)}/session`, () => undefined);
    },
  };
}

export type TunnelClient = ReturnType<typeof createTunnelClient>;

/**
 * Invoke function injected by the host. The Tauri internals `invoke` entry
 * qualifies; so does any test double with the same shape. Host-free: this
 * package never names Tauri itself.
 */
export type InvokeFn = (cmd: string, args?: Record<string, unknown>) => Promise<unknown>;

/**
 * The same typed client over Tauri IPC instead of HTTP. Command names mirror
 * the daemon routes 1:1 (`tunnel_list_profiles` for `GET /api/profiles`,
 * …), responses are validated against the same contract, and failures arrive
 * as the same `{ code, message, retryable }` problem — so a front cannot tell
 * which transport it runs on.
 */
export function createInvokeTunnelClient(invoke: InvokeFn): TunnelClient {
  async function call<T>(
    cmd: string,
    args: Record<string, unknown> | undefined,
    parse: (body: unknown) => T,
  ): Promise<T> {
    let body: unknown;
    try {
      body = await invoke(cmd, args);
    } catch (error) {
      throw problemToError(error);
    }
    try {
      return parse(body);
    } catch {
      const code: TunnelProblemCode = "InternalError";
      throw new TunnelError(code, "host response failed validation");
    }
  }
  return {
    health: () =>
      call("tunnel_health", undefined, (body) =>
        must(healthResponseSchema.loose().safeParse(body)),
      ),
    listProfiles: () =>
      call("tunnel_list_profiles", undefined, (body) =>
        must(profileListResponseSchema.loose().safeParse(body)).profiles,
      ),
    createProfile: (input) =>
      call("tunnel_create_profile", { input }, (body) =>
        must(tunnelProfileSchema.loose().safeParse(body)),
      ),
    getProfile: (id) =>
      call("tunnel_get_profile", { id }, (body) =>
        must(tunnelProfileSchema.loose().safeParse(body)),
      ),
    updateProfile: (id, input) =>
      call("tunnel_update_profile", { id, input }, (body) =>
        must(tunnelProfileSchema.loose().safeParse(body)),
      ),
    deleteProfile: async (id): Promise<void> => {
      await call("tunnel_delete_profile", { id }, () => undefined);
    },
    preview: (id) =>
      call("tunnel_preview", { id }, (body) =>
        must(previewResponseSchema.loose().safeParse(body)),
      ),
    listStates: () =>
      call("tunnel_list_states", undefined, (body) =>
        must(stateListResponseSchema.loose().safeParse(body)).states,
      ),
    getState: (id) =>
      call("tunnel_get_state", { id }, (body) =>
        must(tunnelStateSchema.loose().safeParse(body)),
      ),
    start: (id) =>
      call("tunnel_start", { id }, (body) =>
        must(tunnelStateSchema.loose().safeParse(body)),
      ),
    stop: (id) =>
      call("tunnel_stop", { id }, (body) =>
        must(tunnelStateSchema.loose().safeParse(body)),
      ),
    stopAll: () =>
      call("tunnel_stop_all", undefined, (body) =>
        must(stateListResponseSchema.loose().safeParse(body)).states,
      ),
    probe: (id) =>
      call("tunnel_probe", { id }, (body) =>
        must(tunnelStateSchema.loose().safeParse(body)),
      ),
    listHosts: () =>
      call("tunnel_list_hosts", undefined, (body) =>
        must(hostListResponseSchema.loose().safeParse(body)).hosts,
      ),
    listTerminals: () =>
      call("tunnel_list_terminals", undefined, (body) =>
        must(terminalListResponseSchema.loose().safeParse(body)).terminals,
      ),
    getLog: (id, tail) =>
      call("tunnel_log_tail", { id, tail }, (body) =>
        must(logTailResponseSchema.loose().safeParse(body)),
      ),
    launchInTerminal: (id, terminalId, mode) =>
      call("tunnel_launch_in_terminal", { id, terminalId, mode }, (body) =>
        must(launchResponseSchema.loose().safeParse(body)),
      ),
    openUrl: (url) =>
      call("tunnel_open_url", { url }, (body) =>
        must(openUrlResponseSchema.loose().safeParse(body)),
      ),
    closeSession: async (id): Promise<void> => {
      await call("tunnel_close_session", { id }, () => undefined);
    },
  };
}
