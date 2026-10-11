import type { TimelineItem } from "./timeline";

export type SessionMode = "live_interview" | "mock_interview" | "learning";
export type SessionStatus = "ready" | "active" | "paused" | "completed" | "archived";

export type Session = {
  id: string;
  title: string;
  mode: SessionMode;
  status: SessionStatus;
  context: Record<string, unknown>;
  configuration: Record<string, unknown>;
  created_at: string;
  updated_at: string;
};

export type SessionsByMode = {
  live_interview: Session[];
  mock_interview: Session[];
  learning: Session[];
};

export type CreateSessionInput = {
  title: string;
  mode: SessionMode;
  context?: Record<string, unknown>;
  configuration?: Record<string, unknown>;
};

export class ApiError extends Error {
  constructor(
    message: string,
    readonly status: number,
  ) {
    super(message);
  }
}

const MODES = new Set<SessionMode>(["live_interview", "mock_interview", "learning"]);

export function isSessionMode(value: string): value is SessionMode {
  return MODES.has(value as SessionMode);
}

const NEXT_STATUS: Record<SessionStatus, SessionStatus[]> = {
  ready: ["ready", "active", "archived"],
  active: ["active", "paused", "completed", "archived"],
  paused: ["paused", "active", "archived"],
  completed: ["completed", "archived"],
  archived: ["archived"],
};

export function allowedStatuses(current: SessionStatus): SessionStatus[] {
  return NEXT_STATUS[current];
}

async function request<T>(path: string, init?: RequestInit): Promise<T> {
  const response = await fetch(path, {
    ...init,
    headers: {
      accept: "application/json",
      ...(init?.body ? { "content-type": "application/json" } : {}),
      ...init?.headers,
    },
  });
  if (response.status === 204) {
    return undefined as T;
  }
  const text = await response.text();
  const body = text ? (JSON.parse(text) as { error?: string }) : {};
  if (!response.ok) {
    throw new ApiError(body.error ?? response.statusText, response.status);
  }
  return body as T;
}

export function getHealth(): Promise<{ status: string; app: string; version: string }> {
  return request("/api/health");
}

export function listSessions(mode: SessionMode): Promise<{ sessions: Session[] }> {
  return request(`/api/sessions?mode=${mode}`);
}

export function groupedSessions(): Promise<SessionsByMode> {
  return request("/api/sessions?grouped=true");
}

export function getSession(id: string): Promise<Session> {
  return request(`/api/sessions/${id}`);
}

export function createSession(input: CreateSessionInput): Promise<Session> {
  return request("/api/sessions", { method: "POST", body: JSON.stringify(input) });
}

export function updateSession(
  id: string,
  input: { title?: string; status?: SessionStatus },
): Promise<Session> {
  return request(`/api/sessions/${id}`, { method: "PATCH", body: JSON.stringify(input) });
}

export function deleteSession(id: string): Promise<void> {
  return request(`/api/sessions/${id}`, { method: "DELETE" });
}

export function getTimeline(sessionId: string): Promise<{ items: TimelineItem[] }> {
  return request(`/api/sessions/${sessionId}/timeline`);
}

export function postMessage(sessionId: string, text: string): Promise<TimelineItem> {
  return request(`/api/sessions/${sessionId}/messages`, {
    method: "POST",
    body: JSON.stringify({ kind: "user_message", text, metadata: {} }),
  });
}

export function postAssistantAction(
  sessionId: string,
  utteranceId: string,
): Promise<{ message: TimelineItem }> {
  return request(`/api/sessions/${sessionId}/assistant-actions`, {
    method: "POST",
    body: JSON.stringify({
      action: "explain",
      target: { kind: "utterance", id: utteranceId },
    }),
  });
}
