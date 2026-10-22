import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { FormEvent, useState } from "react";
import { Link, useNavigate, useParams } from "react-router-dom";
import {
  allowedStatuses,
  deleteSession,
  exportSessionBlobUrl,
  getSelection,
  getSession,
  learningTurn,
  mockTurn,
  modelCatalogs,
  postAssistantAction,
  postMessage,
  updateSession,
  type Capability,
  type ModelCatalog,
  type ProviderSelection,
  type SessionMode,
  type SessionStatus,
} from "../api";
import { CaptureControls } from "../components/CaptureControls";
import { Combobox } from "../components/Combobox";
import { ScreenControls } from "../components/ScreenControls";
import { Timeline } from "../components/Timeline";
import { VoiceControls } from "../components/VoiceControls";
import { useTimeline } from "../useTimeline";

const SLUG: Record<SessionMode, string> = {
  live_interview: "live",
  mock_interview: "mock",
  learning: "learning",
};

const PROFILE_CONTEXT_KEY = "talkcer-profile-context";

function storedProfileContext(): boolean {
  return sessionStorage.getItem(PROFILE_CONTEXT_KEY) === "true";
}

type ModelChoice = { provider: string; model: string };

function choicesFor(capability: Capability, catalogs: ModelCatalog[], selection: ProviderSelection | undefined): ModelChoice[] {
  const offered = selection?.offered.filter((item) => item.provider) ?? [];
  const disabled = new Set(selection?.disabled ?? []);
  const byProvider = new Map<string, string[]>();
  for (const catalog of catalogs.filter((item) => item.capability === capability && !disabled.has(item.provider))) {
    const ticked = offered.filter((item) => item.provider === catalog.provider).map((item) => item.model);
    const models = ticked.length > 0 ? ticked : catalog.default_model ? [catalog.default_model] : [];
    if (models.length > 0) byProvider.set(catalog.provider, models);
  }
  for (const item of offered) {
    if (disabled.has(item.provider) || byProvider.has(item.provider)) continue;
    byProvider.set(item.provider, offered.filter((entry) => entry.provider === item.provider).map((entry) => entry.model));
  }
  return [...byProvider.entries()].flatMap(([provider, models]) => models.map((model) => ({ provider, model })));
}

function preferred(choices: ModelChoice[], defaults: Record<string, string> | undefined): string {
  const match = choices.find((choice) => defaults?.[choice.provider] === choice.model);
  const first = match ?? choices[0];
  return first ? `${first.provider}:${first.model}` : "";
}

export function SessionDetailPage() {
  const { id = "" } = useParams();
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const session = useQuery({ queryKey: ["session", id], queryFn: () => getSession(id) });
  const timeline = useTimeline(id);
  const catalogs = useQuery({ queryKey: ["models"], queryFn: modelCatalogs });
  const selections = useQuery({ queryKey: ["selection"], queryFn: getSelection });
  const [title, setTitle] = useState<string | null>(null);
  const [message, setMessage] = useState("");
  const [model, setModel] = useState("");
  const [sttModel, setSttModel] = useState("");
  const [ttsModel, setTtsModel] = useState("");
  const [profileContext, setProfileContext] = useState(storedProfileContext);
  const [error, setError] = useState<string | null>(null);
  const [exportUrl, setExportUrl] = useState<string | null>(null);
  const [exportName, setExportName] = useState("talkcer-session.md");

  async function downloadExport(format: "json" | "markdown") {
    try {
      const url = await exportSessionBlobUrl(id, format);
      if (exportUrl) {
        URL.revokeObjectURL(exportUrl);
      }
      setExportUrl(url);
      setExportName(format === "json" ? `talkcer-session-${id}.json` : `talkcer-session-${id}.md`);
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : "Export failed.");
    }
  }

  const save = useMutation({
    mutationFn: (next: { title?: string; status?: SessionStatus; configuration?: Record<string, unknown> }) =>
      updateSession(id, next),
    onSuccess: async () => {
      setError(null);
      await queryClient.invalidateQueries({ queryKey: ["session", id] });
      await queryClient.invalidateQueries({ queryKey: ["sessions"] });
    },
    onError: (cause: Error) => setError(cause.message),
  });

  const remove = useMutation({
    mutationFn: () => deleteSession(id),
    onSuccess: async () => {
      const mode = session.data?.mode ?? "live_interview";
      await queryClient.invalidateQueries({ queryKey: ["sessions"] });
      await navigate(`/${SLUG[mode]}`);
    },
    onError: (cause: Error) => setError(cause.message),
  });

  const send = useMutation({
    mutationFn: () => postMessage(id, message, { model: model || undefined, profile_context: profileContext }),
    onSuccess: async () => {
      setMessage("");
      setError(null);
      await queryClient.invalidateQueries({ queryKey: ["timeline", id] });
    },
    onError: (cause: Error) => setError(cause.message),
  });

  const mock = useMutation({
    mutationFn: (input: { action: "start" | "answer" | "complete"; text?: string }) =>
      mockTurn(id, input.action, input.text),
    onSuccess: async () => {
      setMessage("");
      setError(null);
      await queryClient.invalidateQueries({ queryKey: ["session", id] });
      await queryClient.invalidateQueries({ queryKey: ["timeline", id] });
      await queryClient.invalidateQueries({ queryKey: ["sessions"] });
    },
    onError: (cause: Error) => setError(cause.message),
  });

  const learning = useMutation({
    mutationFn: (input: { action: "start" | "discuss" | "answer"; text?: string }) =>
      learningTurn(id, input.action, input.text),
    onSuccess: async () => {
      setMessage("");
      setError(null);
      await queryClient.invalidateQueries({ queryKey: ["session", id] });
      await queryClient.invalidateQueries({ queryKey: ["timeline", id] });
      await queryClient.invalidateQueries({ queryKey: ["sessions"] });
    },
    onError: (cause: Error) => setError(cause.message),
  });

  const explain = useMutation({
    mutationFn: (utteranceId: string) => postAssistantAction(id, utteranceId),
    onSuccess: async () => {
      setError(null);
      await queryClient.invalidateQueries({ queryKey: ["timeline", id] });
    },
    onError: (cause: Error) => setError(cause.message),
  });

  if (session.isLoading) {
    return <p className="muted">Loading session…</p>;
  }
  if (session.isError || !session.data) {
    return <p className="error">Session not found.</p>;
  }

  const current = session.data;
  const draftTitle = title ?? current.title;
  const spoken = current.mode === "mock_interview" || current.mode === "learning";
  const selectionFor = (capability: Capability) => selections.data?.find((item) => item.capability === capability);
  const thinkerChoices = choicesFor("thinker", catalogs.data ?? [], selectionFor("thinker"));
  const sttChoices = choicesFor("stt", catalogs.data ?? [], selectionFor("stt"));
  const ttsChoices = choicesFor("tts", catalogs.data ?? [], selectionFor("tts"));
  const thinkerModel = model || preferred(thinkerChoices, selectionFor("thinker")?.defaults);
  const speechModel = sttModel || String(current.configuration.stt_model ?? "") || preferred(sttChoices, selectionFor("stt")?.defaults);
  const voiceModel = ttsModel || String(current.configuration.tts_model ?? "") || preferred(ttsChoices, selectionFor("tts")?.defaults);

  function rememberModel(slot: "stt_model" | "tts_model", value: string) {
    if (slot === "stt_model") setSttModel(value);
    else setTtsModel(value);
    save.mutate({ configuration: { ...current.configuration, [slot]: value } });
  }

  function onSave(event: FormEvent) {
    event.preventDefault();
    save.mutate({ title: draftTitle });
  }

  function onSend(event: FormEvent) {
    event.preventDefault();
    send.mutate();
  }

  return (
    <section className="desk">
      <header className="desk-top">
        <div>
          <p className="kicker">
            <Link to={`/${SLUG[current.mode]}`}>Back</Link>
            {" · "}
            {current.mode}
          </p>
          <h2>{current.title}</h2>
          <p className="desk-status">
            <span className="live-dot" data-on={String(timeline.connection === "open")} />
            {current.status} · {timeline.connection === "open" ? "Connected" : "Reconnecting…"}
          </p>
        </div>
        <div className="actions">
          {(current.status === "ready" || current.status === "paused") && (
            <button
              type="button"
              className="primary"
              onClick={() => save.mutate({ status: "active" })}
              disabled={save.isPending}
            >
              {current.status === "paused" ? "Resume" : "Start"}
            </button>
          )}
          {current.status === "active" && (
            <button
              type="button"
              className="primary"
              onClick={() => save.mutate({ status: "paused" })}
              disabled={save.isPending}
            >
              Stop
            </button>
          )}
          {current.mode === "mock_interview" && current.status === "ready" && (
            <button type="button" className="primary" onClick={() => mock.mutate({ action: "start" })} disabled={mock.isPending}>
              Start mock interview
            </button>
          )}
          {current.mode === "learning" && current.status === "ready" && (
            <button type="button" className="primary" onClick={() => learning.mutate({ action: "start" })} disabled={learning.isPending}>
              Start learning
            </button>
          )}
          <span className="note">One session runs at a time.</span>
        </div>
      </header>
      <div className="transcript">
        {timeline.isLoading && <p className="muted">Loading timeline…</p>}
        {timeline.isError && <p className="error">Could not load timeline.</p>}
        {!timeline.isLoading && !timeline.isError && (
          <Timeline
            items={timeline.items}
            explaining={explain.isPending}
            onExplain={(utteranceId) => explain.mutate(utteranceId)}
          />
        )}
      </div>
      <div className="composer-dock">
        <div className="composer">
        {current.mode === "mock_interview" && current.status === "active" && (
          <form
            onSubmit={(event) => {
              event.preventDefault();
              mock.mutate({ action: "answer", text: message });
            }}
          >
            <label>
              Answer
              <input value={message} onChange={(event) => setMessage(event.target.value)} />
            </label>
            <button type="submit" className="primary" disabled={mock.isPending || message.trim().length === 0}>
              Send answer
            </button>
            <button
              type="button"
              disabled={mock.isPending || message.trim().length === 0}
              onClick={() => mock.mutate({ action: "complete", text: message })}
            >
              Complete interview
            </button>
          </form>
        )}
        {current.mode === "learning" && current.status === "active" && (
          <form
            onSubmit={(event) => {
              event.preventDefault();
              learning.mutate({ action: "discuss", text: message });
            }}
          >
            <label>
              Question
              <input value={message} onChange={(event) => setMessage(event.target.value)} />
            </label>
            <button type="submit" className="primary" disabled={learning.isPending || message.trim().length === 0}>
              Ask
            </button>
            <button
              type="button"
              disabled={learning.isPending || message.trim().length === 0}
              onClick={() => learning.mutate({ action: "answer", text: message })}
            >
              Answer practice
            </button>
          </form>
        )}
        <div className="composer-options">
          <ModelSelect label="Model" value={thinkerModel} choices={thinkerChoices} onChange={setModel} />
          {spoken && <ModelSelect label="Speech to text" value={speechModel} choices={sttChoices} onChange={(value) => rememberModel("stt_model", value)} />}
          {spoken && <ModelSelect label="Voice" value={voiceModel} choices={ttsChoices} onChange={(value) => rememberModel("tts_model", value)} />}
          <label className="check">
            <input
              type="checkbox"
              checked={profileContext}
              onChange={(event) => {
                setProfileContext(event.target.checked);
                sessionStorage.setItem(PROFILE_CONTEXT_KEY, String(event.target.checked));
              }}
            />
            Add profile context
          </label>
        </div>
        <form className="composer-form" onSubmit={onSend}>
          <input
            aria-label="Message"
            placeholder="Message"
            value={message}
            onChange={(event) => setMessage(event.target.value)}
          />
          <button type="submit" className="primary" disabled={send.isPending || message.trim().length === 0}>
            {send.isPending ? "Sending…" : "Send"}
          </button>
        </form>
        </div>
        <details className="drawer">
          <summary>Session tools</summary>
          <div className="drawer-body">
          {(current.mode === "mock_interview" || current.mode === "learning") && (
            <VoiceControls
              session={current}
              items={timeline.items}
              onChange={async () => {
                await queryClient.invalidateQueries({ queryKey: ["session", id] });
                await queryClient.invalidateQueries({ queryKey: ["timeline", id] });
              }}
            />
          )}
          {current.mode === "live_interview" && (
            <CaptureControls
              sessionId={current.id}
              onChange={async () => {
                await queryClient.invalidateQueries({ queryKey: ["timeline", id] });
              }}
            />
          )}
          <ScreenControls
            sessionId={current.id}
            onChange={async () => {
              await queryClient.invalidateQueries({ queryKey: ["timeline", id] });
            }}
          />
          <p className="note">Starting this session pauses any other running session.</p>
          <div className="session-tools">
        {error && <p className="error">{error}</p>}
        <div className="row">
          <button type="button" onClick={() => void downloadExport("markdown")}>Download Markdown</button>
          <button type="button" onClick={() => void downloadExport("json")}>Download JSON</button>
          {exportUrl && <a href={exportUrl} download={exportName}>Save {exportName}</a>}
        </div>
        <form onSubmit={onSave}>
          <label>
            Title
            <input value={draftTitle} onChange={(event) => setTitle(event.target.value)} />
          </label>
          <label>
            Status
            <select
              value={current.status}
              onChange={(event) => save.mutate({ status: event.target.value as SessionStatus })}
            >
              {allowedStatuses(current.status).map((status) => (
                <option key={status} value={status}>
                  {status}
                </option>
              ))}
            </select>
          </label>
          <button type="submit" className="primary" disabled={save.isPending}>
            {save.isPending ? "Saving…" : "Save title"}
          </button>
        </form>
        <button type="button" onClick={() => remove.mutate()} disabled={remove.isPending}>
          {remove.isPending ? "Deleting…" : "Delete session"}
        </button>
          </div>
          </div>
        </details>
      </div>
    </section>
  );
}

function ModelSelect({
  label,
  value,
  choices,
  onChange,
}: {
  label: string;
  value: string;
  choices: ModelChoice[];
  onChange: (value: string) => void;
}) {
  return (
    <Combobox
      className="pick"
      label={label}
      value={value}
      options={choices.map((choice) => ({
        value: `${choice.provider}:${choice.model}`,
        label: choice.model,
        group: choice.provider,
      }))}
      onChange={onChange}
    />
  );
}
