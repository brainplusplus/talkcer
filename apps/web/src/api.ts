export type { TimelineItem } from "./timeline";
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
  return request(listSessionsPath(mode));
}

export function groupedSessions(): Promise<SessionsByMode> {
  return request("/api/sessions?grouped=true");
}

export function listSessionsPath(mode: SessionMode): string {
  return `/api/sessions?mode=${mode}`;
}

export function sessionPath(id: string): string {
  return `/api/sessions/${id}`;
}

export function timelinePath(sessionId: string): string {
  return `/api/sessions/${sessionId}/timeline`;
}

export function getSession(id: string): Promise<Session> {
  return request(sessionPath(id));
}

export function createSession(input: CreateSessionInput): Promise<Session> {
  return request("/api/sessions", { method: "POST", body: JSON.stringify(input) });
}

export function updateSession(
  id: string,
  input: { title?: string; status?: SessionStatus; configuration?: Record<string, unknown> },
): Promise<Session> {
  return request(`/api/sessions/${id}`, { method: "PATCH", body: JSON.stringify(input) });
}

export function deleteSession(id: string): Promise<void> {
  return request(`/api/sessions/${id}`, { method: "DELETE" });
}

export function getTimeline(sessionId: string): Promise<{ items: TimelineItem[] }> {
  return request(timelinePath(sessionId));
}

export function postMessage(sessionId: string, text: string): Promise<TimelineItem> {
  return request(`/api/sessions/${sessionId}/messages`, {
    method: "POST",
    body: JSON.stringify({ kind: "user_message", text, metadata: {} }),
  });
}

export type ProviderKind = "embedded" | "self_hosted" | "cloud";
export type Capability = "stt" | "thinker" | "tts";

export type ProviderConfig = {
  kind: ProviderKind;
  provider: string;
  endpoint?: string;
  model: string;
  credentials_ref?: string;
  consent?: boolean;
};

export type AiConfiguration = {
  id: string;
  name: string;
  stt: ProviderConfig;
  thinker: ProviderConfig;
  tts: ProviderConfig;
};

export type ProviderDescriptor = {
  capability: Capability;
  provider: string;
  kind: ProviderKind;
  capabilities: {
    supports_streaming: boolean;
    supports_partial_transcript: boolean;
    supports_word_timestamps: boolean;
    supports_cancellation: boolean;
    max_sample_rate: number | null;
    supported_languages: string[];
  };
};

export function getAiConfigurations(): Promise<AiConfiguration[]> {
  return request("/api/config/ai");
}

export function getProviders(): Promise<ProviderDescriptor[]> {
  return request("/api/config/ai/providers");
}

export function saveAiConfiguration(input: {
  id?: string;
  name: string;
  stt: ProviderConfig;
  thinker: ProviderConfig;
  tts: ProviderConfig;
}): Promise<AiConfiguration> {
  return request("/api/config/ai", { method: "PUT", body: JSON.stringify(input) });
}

export function testProvider(id: string, capability: Capability): Promise<{ ok: boolean; message: string }> {
  return request(`/api/config/ai/${id}/test`, {
    method: "POST",
    body: JSON.stringify({ capability }),
  });
}

export function learningTurn(
  sessionId: string,
  action: "start" | "discuss" | "answer",
  text?: string,
): Promise<{ status: SessionStatus; items: TimelineItem[] }> {
  return request(`/api/sessions/${sessionId}/learning`, {
    method: "POST",
    body: JSON.stringify({ action, text }),
  });
}

export function sendVoiceAudio(
  sessionId: string,
  input: { utteranceId?: string; audioSource: "browser_mic" | "mock_audio"; text?: string; isFinal: boolean },
): Promise<{ item: TimelineItem }> {
  return request(`/api/sessions/${sessionId}/voice/audio`, {
    method: "POST",
    body: JSON.stringify({
      utterance_id: input.utteranceId,
      audio_source: input.audioSource,
      text: input.text,
      started_at_ms: Date.now(),
      is_final: input.isFinal,
    }),
  });
}

export type CaptureDevice = {
  channel: string;
  name: string;
  direction: string;
  state: string;
  sample_rate: number;
  channels: number;
  frames: number;
};

export type CaptureStatus = {
  state: string;
  platform: string;
  error: string | null;
  devices?: CaptureDevice[];
};

export function captureControl(sessionId: string, action: "start" | "pause" | "resume" | "stop" | "disconnect" | "recover"): Promise<CaptureStatus> {
  return request(capturePath(sessionId), { method: "POST", body: JSON.stringify({ action }) });
}

export function capturePath(sessionId: string): string {
  return `/api/sessions/${sessionId}/capture`;
}

export function captureDevicesPath(sessionId: string): string {
  return `/api/sessions/${sessionId}/capture/devices`;
}

export function audioDevicesPath(): string {
  return "/api/devices/audio";
}

export function audioDevices(): Promise<{ devices: CaptureDevice[] }> {
  return request(audioDevicesPath());
}

export function captureDevices(sessionId: string): Promise<{ devices: CaptureDevice[] }> {
  return request(captureDevicesPath(sessionId));
}

export function injectCapture(sessionId: string, channel: "microphone" | "system_audio", text: string): Promise<{ item: TimelineItem }> {
  return request(`/api/sessions/${sessionId}/capture/fixture`, {
    method: "POST",
    body: JSON.stringify({ channel, text, started_at_ms: Date.now(), is_final: true }),
  });
}

export function remoteStatus(): Promise<{ enabled: boolean }> {
  return request("/api/remote/status");
}

export function pairRemote(enabled: boolean): Promise<{ enabled: boolean; token?: string }> {
  return request("/api/remote/pair", { method: "POST", body: JSON.stringify({ enabled }) });
}

export function remoteSessions(token: string): Promise<{ sessions: Session[] }> {
  return request("/api/sessions", {
    headers: { authorization: `Bearer ${token}`, "x-talkcer-client": "remote" },
  });
}

export function speakText(sessionId: string, text: string): Promise<{ enabled: boolean; audio_base64: string | null }> {
  return request(`/api/sessions/${sessionId}/voice/speak`, {
    method: "POST",
    body: JSON.stringify({ text }),
  });
}

export function mockTurn(
  sessionId: string,
  action: "start" | "answer" | "complete",
  text?: string,
): Promise<{ status: SessionStatus; items: TimelineItem[] }> {
  return request(`/api/sessions/${sessionId}/mock`, {
    method: "POST",
    body: JSON.stringify({ action, text }),
  });
}

export type OverlayPreferences = {
  state: "hidden" | "visible" | "click_through";
  always_on_top: boolean;
  opacity: number;
  position_x: number;
  position_y: number;
  active_session_id: string | null;
};

export function overlayViewPath(): string {
  return "/api/overlay/view";
}

export function exportSessionPath(sessionId: string, format: "json" | "markdown"): string {
  return `/api/sessions/${sessionId}/export?format=${format}`;
}

export function getOverlay(): Promise<OverlayPreferences> {
  return request("/api/overlay");
}

export function showOverlay(): Promise<OverlayPreferences> {
  return request("/api/overlay/show", { method: "POST" });
}

export function hideOverlay(): Promise<OverlayPreferences> {
  return request("/api/overlay/hide", { method: "POST" });
}

export function toggleOverlay(): Promise<OverlayPreferences> {
  return request("/api/overlay/toggle", { method: "POST" });
}

export function moveOverlay(x: number, y: number): Promise<OverlayPreferences> {
  return request("/api/overlay/move", { method: "POST", body: JSON.stringify({ x, y }) });
}

export function setOverlayOpacity(opacity: number): Promise<OverlayPreferences> {
  return request("/api/overlay/opacity", { method: "POST", body: JSON.stringify({ opacity }) });
}

export function setOverlayClickThrough(enabled: boolean): Promise<OverlayPreferences> {
  return request("/api/overlay/click-through", { method: "POST", body: JSON.stringify({ enabled }) });
}

export function followOverlaySession(sessionId: string): Promise<OverlayPreferences> {
  return request("/api/overlay", { method: "PUT", body: JSON.stringify({ session_id: sessionId }) });
}

export function overlayView(): Promise<{ items: TimelineItem[] }> {
  return request(overlayViewPath());
}

export async function exportSessionBlobUrl(sessionId: string, format: "json" | "markdown"): Promise<string> {
  const response = await fetch(exportSessionPath(sessionId, format));
  if (!response.ok) {
    throw new ApiError(await response.text(), response.status);
  }
  const blob = await response.blob();
  return URL.createObjectURL(blob);
}

export async function exportSession(sessionId: string, format: "json" | "markdown"): Promise<string> {
  const response = await fetch(exportSessionPath(sessionId, format));
  const text = await response.text();
  if (!response.ok) {
    throw new ApiError(text, response.status);
  }
  return text;
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
