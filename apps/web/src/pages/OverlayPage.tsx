import { FormEvent, useState } from "react";
import { useMutation, useQuery } from "@tanstack/react-query";
import {
  exportSession,
  followOverlaySession,
  getOverlay,
  hideOverlay,
  moveOverlay,
  overlayView,
  setOverlayClickThrough,
  setOverlayOpacity,
  showOverlay,
} from "../api";

export function OverlayPage() {
  const overlay = useQuery({ queryKey: ["overlay"], queryFn: getOverlay });
  const view = useQuery({ queryKey: ["overlay-view"], queryFn: overlayView, enabled: overlay.data?.state !== "hidden" });
  const [sessionId, setSessionId] = useState("");
  const [exportText, setExportText] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const change = useMutation({
    mutationFn: (action: () => Promise<unknown>) => action(),
    onSuccess: async () => {
      setError(null);
      await overlay.refetch();
      await view.refetch();
    },
    onError: (cause: Error) => setError(cause.message),
  });

  function follow(event: FormEvent) {
    event.preventDefault();
    change.mutate(() => followOverlaySession(sessionId));
  }

  return (
    <section className="stack">
      <h2>Overlay</h2>
      <p className="meta">
        {overlay.data ? `${overlay.data.state} · opacity ${overlay.data.opacity} · on top ${overlay.data.always_on_top}` : "Loading overlay…"}
      </p>
      <div className="row">
        <button type="button" onClick={() => change.mutate(showOverlay)}>Show</button>
        <button type="button" onClick={() => change.mutate(hideOverlay)}>Hide</button>
        <button type="button" onClick={() => change.mutate(() => moveOverlay(80, 120))}>Move</button>
        <button type="button" onClick={() => change.mutate(() => setOverlayOpacity(0.6))}>Opacity</button>
        <button type="button" onClick={() => change.mutate(() => setOverlayClickThrough(true))}>Click-through</button>
      </div>
      <form onSubmit={follow} className="row">
        <input value={sessionId} onChange={(event) => setSessionId(event.target.value)} placeholder="Active session id" />
        <button type="submit">Follow session</button>
      </form>
      {overlay.data?.active_session_id && (
        <div className="row">
          <button type="button" onClick={() => change.mutate(async () => setExportText(await exportSession(overlay.data!.active_session_id!, "markdown")))}>
            Export Markdown
          </button>
          <button type="button" onClick={() => change.mutate(async () => setExportText(await exportSession(overlay.data!.active_session_id!, "json")))}>
            Export JSON
          </button>
        </div>
      )}
      {view.data?.items.map((item) => (
        <p key={item.id}>{item.speaker}: {item.text}</p>
      ))}
      {exportText && <pre>{exportText}</pre>}
      {error && <p className="error">{error}</p>}
    </section>
  );
}
