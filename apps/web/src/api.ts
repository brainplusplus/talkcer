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
  const response = await fetch(apiUrl(path), {
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

/**
 * Where the API lives for this page.
 *
 * Served by Vite (dev) or the same origin (web), the relative path is kept.
 * Inside the desktop shell the page origin is `http://tauri.localhost`, which
 * has no server, so requests go to the local core API explicitly.
 *
 * Pure: exported so tests can cover every origin without a browser.
 */
export function resolveApiBase(origin: {
  host: string;
  protocol: string;
  configured?: string;
}): string {
  if (origin.configured) {
    return trimBase(origin.configured);
  }
  if (origin.host === "tauri.localhost" || origin.protocol === "tauri:") {
    return "http://127.0.0.1:54000";
  }
  return "";
}

export function apiBase(): string {
  if (typeof window === "undefined") {
    return "";
  }
  return resolveApiBase({
    host: window.location.host,
    protocol: window.location.protocol,
    configured: import.meta.env.VITE_TALKCER_API,
  });
}

function trimBase(base: string): string {
  return base.replace(/\/+$/, "");
}

/**
 * WebSocket origin for this page, mirroring {@link apiBase}.
 *
 * `""` means same-origin (Vite dev proxy or a server that hosts the web app);
 * inside the desktop shell it is the local core API origin.
 */
export function wsBase(): string {
  const base = apiBase();
  if (!base) {
    return "";
  }
  return base.replace(/^http/, "ws");
}

function apiUrl(path: string): string {
  const base = apiBase();
  if (!base) {
    return path;
  }
  return `${base}${path.startsWith("/") ? path : `/${path}`}`;
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

export function captureScreen(sessionId: string, direct: boolean): Promise<{ reply: TimelineItem | null }> {
  return request(`/api/sessions/${sessionId}/screen`, {
    method: "POST",
    body: JSON.stringify({ direct }),
  });
}

export function sendScreen(sessionId: string, note: string): Promise<TimelineItem> {
  return request(`/api/sessions/${sessionId}/screen/send`, {
    method: "POST",
    body: JSON.stringify({ note }),
  });
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
  undetectable: boolean;
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

export function setUndetectable(enabled: boolean): Promise<OverlayPreferences> {
  return request("/api/overlay/undetectable", { method: "POST", body: JSON.stringify({ enabled }) });
}

export function followOverlaySession(sessionId: string): Promise<OverlayPreferences> {
  return request("/api/overlay", { method: "PUT", body: JSON.stringify({ session_id: sessionId }) });
}

export function overlayView(): Promise<{ items: TimelineItem[] }> {
  return request(overlayViewPath());
}

export async function exportSessionBlobUrl(sessionId: string, format: "json" | "markdown"): Promise<string> {
  const response = await fetch(apiUrl(exportSessionPath(sessionId, format)));
  if (!response.ok) {
    throw new ApiError(await response.text(), response.status);
  }
  const blob = await response.blob();
  return URL.createObjectURL(blob);
}

export async function exportSession(sessionId: string, format: "json" | "markdown"): Promise<string> {
  const response = await fetch(apiUrl(exportSessionPath(sessionId, format)));
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

export type AssistantActionName = "explain" | "translate" | "suggest_points";

export function postNamedAssistantAction(
  sessionId: string,
  utteranceId: string,
  action: AssistantActionName,
): Promise<{ message: TimelineItem }> {
  return request(`/api/sessions/${sessionId}/assistant-actions`, {
    method: "POST",
    body: JSON.stringify({
      action,
      target: { kind: "utterance", id: utteranceId },
    }),
  });
}

/**
 * The desktop shell bridge.
 *
 * Inside the Tauri shell the Rust side applies window commands after every
 * change, so the overlay window moves and hides for real. In a browser the
 * bridge is absent and the API calls are still made; only the window itself
 * cannot move.
 */
type TauriInvoke = <T>(command: string, args?: Record<string, unknown>) => Promise<T>;

function desktopInvoke(): TauriInvoke | null {
  if (typeof window === "undefined") {
    return null;
  }
  const scope = window as unknown as {
    __TAURI__?: { core?: { invoke?: TauriInvoke } };
  };
  return scope.__TAURI__?.core?.invoke ?? null;
}

export function isDesktopShell(): boolean {
  return desktopInvoke() !== null;
}

async function invokeDesktop(command: string, args?: Record<string, unknown>): Promise<void> {
  const invoke = desktopInvoke();
  if (!invoke) {
    return;
  }
  await invoke(command, args);
}

export async function showOverlayDesktop(): Promise<OverlayPreferences> {
  const preferences = await showOverlay();
  await invokeDesktop("overlay_toggle");
  return preferences;
}

export async function hideOverlayDesktop(): Promise<OverlayPreferences> {
  const preferences = await hideOverlay();
  await invokeDesktop("overlay_hide");
  return preferences;
}

export async function moveOverlayDesktop(x: number, y: number): Promise<OverlayPreferences> {
  const preferences = await moveOverlay(x, y);
  await invokeDesktop("overlay_move", { x, y });
  return preferences;
}

export async function setOverlayOpacityDesktop(opacity: number): Promise<OverlayPreferences> {
  const preferences = await setOverlayOpacity(opacity);
  await invokeDesktop("overlay_opacity", { opacity });
  return preferences;
}

export async function setOverlayClickThroughDesktop(enabled: boolean): Promise<OverlayPreferences> {
  const preferences = await setOverlayClickThrough(enabled);
  await invokeDesktop("overlay_click_through", { enabled });
  return preferences;
}

export async function followOverlaySessionDesktop(sessionId: string): Promise<OverlayPreferences> {
  const preferences = await followOverlaySession(sessionId);
  await invokeDesktop("overlay_follow", { sessionId });
  return preferences;
}

export async function overlayQuickAction(action: "explain" | "translate" | "suggest" | "hide"): Promise<void> {
  await invokeDesktop("overlay_quick_action", { action });
}
