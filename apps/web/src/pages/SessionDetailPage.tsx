import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { FormEvent, useState } from "react";
import { Link, useNavigate, useParams } from "react-router-dom";
import {
  allowedStatuses,
  deleteSession,
  getSession,
  postAssistantAction,
  postMessage,
  updateSession,
  type SessionMode,
  type SessionStatus,
} from "../api";
import { Timeline } from "../components/Timeline";
import { useTimeline } from "../useTimeline";

const SLUG: Record<SessionMode, string> = {
  live_interview: "live",
  mock_interview: "mock",
  learning: "learning",
};

export function SessionDetailPage() {
  const { id = "" } = useParams();
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const session = useQuery({ queryKey: ["session", id], queryFn: () => getSession(id) });
  const timeline = useTimeline(id);
  const [title, setTitle] = useState<string | null>(null);
  const [message, setMessage] = useState("");
  const [error, setError] = useState<string | null>(null);

  const save = useMutation({
    mutationFn: (next: { title?: string; status?: SessionStatus }) => updateSession(id, next),
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
    mutationFn: () => postMessage(id, message),
    onSuccess: async () => {
      setMessage("");
      setError(null);
      await queryClient.invalidateQueries({ queryKey: ["timeline", id] });
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

  function onSave(event: FormEvent) {
    event.preventDefault();
    save.mutate({ title: draftTitle });
  }

  function onSend(event: FormEvent) {
    event.preventDefault();
    send.mutate();
  }

  return (
    <section className="stack">
      <p>
        <Link to={`/${SLUG[current.mode]}`}>Back</Link>
      </p>
      <h2>{current.title}</h2>
      <p className="meta">
        {current.mode} · {current.status} · {timeline.connection === "open" ? "Connected" : "Reconnecting…"}
      </p>
      {timeline.isLoading && <p className="muted">Loading timeline…</p>}
      {timeline.isError && <p className="error">Could not load timeline.</p>}
      {!timeline.isLoading && !timeline.isError && (
        <Timeline
          items={timeline.items}
          explaining={explain.isPending}
          onExplain={(utteranceId) => explain.mutate(utteranceId)}
        />
      )}
      <form onSubmit={onSend}>
        <label>
          Message
          <input value={message} onChange={(event) => setMessage(event.target.value)} />
        </label>
        <button type="submit" disabled={send.isPending || message.trim().length === 0}>
          {send.isPending ? "Sending…" : "Send"}
        </button>
      </form>
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
        <button type="submit" disabled={save.isPending}>
          {save.isPending ? "Saving…" : "Save title"}
        </button>
      </form>
      {error && <p className="error">{error}</p>}
      <button type="button" onClick={() => remove.mutate()} disabled={remove.isPending}>
        {remove.isPending ? "Deleting…" : "Delete session"}
      </button>
    </section>
  );
}
