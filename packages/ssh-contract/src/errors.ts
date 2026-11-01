/**
 * Closed problem code table for the SSH tunnel manager.
 *
 * Every failure a client can see is one of these codes plus a human message.
 * The set is closed on purpose: clients branch on `code`, never on message
 * text, and one code always maps to one HTTP status (see STATUS_BY_CODE).
 */
export const TUNNEL_PROBLEM_CODES = [
  /** The request shape or a field value is malformed. */
  "InvalidRequest",
  /** No profile/tunnel with that id. */
  "NotFound",
  /** State forbids the action (e.g. start an already-running tunnel). */
  "Conflict",
  /** Too many concurrent operations; retry later. */
  "ResourceBusy",
  /** A configured limit was exceeded (profiles, forwards, body size). */
  "LimitExceeded",
  /** The `ssh` executable is missing or not runnable. */
  "SshNotFound",
  /** `ssh` exited non-zero or the control check failed. Stderr tail in details. */
  "SshFailed",
  /** A local bind port is already taken by another process. */
  "PortInUse",
  /** A health probe could not reach the forwarded endpoint. */
  "ProbeFailed",
  /** The JSON store could not be read or written. */
  "StoreFailed",
  /** A terminal app refused the launch; its own stderr is in the message. */
  "TerminalFailed",
  /** The operation is not supported by this build/platform. */
  "UnsupportedOperation",
  /** A deadline elapsed; the outcome is unknown, never silently retried. */
  "Timeout",
  /** A defect in this service. Includes a correlation id, not a stack trace. */
  "InternalError",
] as const;

export type TunnelProblemCode =
  (typeof TUNNEL_PROBLEM_CODES)[number];

export const TUNNEL_STATUS_BY_CODE: Record<TunnelProblemCode, number> = {
  InvalidRequest: 400,
  NotFound: 404,
  Conflict: 409,
  ResourceBusy: 429,
  LimitExceeded: 429,
  SshNotFound: 503,
  SshFailed: 502,
  PortInUse: 409,
  ProbeFailed: 502,
  StoreFailed: 500,
  TerminalFailed: 502,
  UnsupportedOperation: 400,
  Timeout: 504,
  InternalError: 500,
};

export interface TunnelProblem {
  readonly code: TunnelProblemCode;
  readonly message: string;
  /** Flat, redacted facts only: field names, ports, exit codes. Never secrets. */
  readonly details?: Readonly<Record<string, string | number | boolean>>;
  readonly retryable: boolean;
}

export interface TunnelProblemResponse {
  readonly problem: TunnelProblem;
}

/** Code-carrying error thrown inside the service/host layers. */
export class TunnelError extends Error {
  readonly code: TunnelProblemCode;
  readonly details?: Readonly<Record<string, string | number | boolean>> | undefined;
  readonly retryable: boolean;

  constructor(
    code: TunnelProblemCode,
    message: string,
    options?: {
      readonly details?: Readonly<Record<string, string | number | boolean>>;
      readonly retryable?: boolean;
    },
  ) {
    super(message);
    this.name = "TunnelError";
    this.code = code;
    this.details = options?.details;
    this.retryable = options?.retryable ?? false;
  }

  toProblem(): TunnelProblem {
    const base: TunnelProblem = {
      code: this.code,
      message: this.message,
      retryable: this.retryable,
    };
    if (this.details === undefined) return base;
    return { ...base, details: this.details };
  }
}

/** Shape guard (no instanceof: it must survive serialization boundaries). */
export function isTunnelProblem(value: unknown): value is TunnelProblem {
  if (typeof value !== "object" || value === null) return false;
  if (!("code" in value) || !("message" in value)) return false;
  const code = (value as { code: unknown }).code;
  const message = (value as { message: unknown }).message;
  return (
    typeof code === "string" &&
    (TUNNEL_PROBLEM_CODES as readonly string[]).includes(code) &&
    typeof message === "string"
  );
}
