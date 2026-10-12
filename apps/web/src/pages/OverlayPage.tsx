import { FormEvent, useState } from "react";
import { useMutation, useQuery } from "@tanstack/react-query";
import {
  exportSessionBlobUrl,
  followOverlaySessionDesktop,
  getOverlay,
  hideOverlayDesktop,
  isDesktopShell,
  moveOverlayDesktop,
  overlayView,
  setOverlayClickThroughDesktop,
  setOverlayOpacityDesktop,
  showOverlayDesktop,
  toggleOverlay,
} from "../api";

export function OverlayPage() {
  const overlay = useQuery({ queryKey: ["overlay"], queryFn: getOverlay });
  const view = useQuery({ queryKey: ["overlay-view"], queryFn: overlayView, enabled: overlay.data?.state !== "hidden" });
  const [sessionId, setSessionId] = useState("");
  const [exportUrl, setExportUrl] = useState<string | null>(null);
  const [exportName, setExportName] = useState("talkcer-session.md");
  const [error, setError] = useState<string | null>(null);
  const [x, setX] = useState("80");
  const [y, setY] = useState("120");
  const [opacity, setOpacityState] = useState("0.6");

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
    change.mutate(() => followOverlaySessionDesktop(sessionId));
  }

  function move(event: FormEvent) {
    event.preventDefault();
    change.mutate(() => moveOverlayDesktop(Number.parseInt(x, 10), Number.parseInt(y, 10)));
  }

  function setOpacity(event: FormEvent) {
    event.preventDefault();
    change.mutate(() => setOverlayOpacityDesktop(Number.parseFloat(opacity)));
  }

  async function download(format: "json" | "markdown") {
    const id = overlay.data?.active_session_id;
    if (!id) {
      return;
    }
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

  return (
    <section className="stack">
      <h2>Overlay</h2>
      {overlay.isLoading && <p className="muted">Loading overlay…</p>}
      {overlay.isError && <p className="error">Could not load overlay preferences.</p>}
      <p className="meta">
        {overlay.data
          ? `${overlay.data.state} · opacity ${overlay.data.opacity} · on top ${overlay.data.always_on_top} · at ${overlay.data.position_x},${overlay.data.position_y}`
          : "Waiting for preferences…"}
      </p>
      <div className="row">
        <button type="button" disabled={change.isPending} onClick={() => change.mutate(showOverlayDesktop)}>Show</button>
        <button type="button" disabled={change.isPending} onClick={() => change.mutate(hideOverlayDesktop)}>Hide</button>
        <button type="button" disabled={change.isPending} onClick={() => change.mutate(toggleOverlay)}>Toggle</button>
        <button type="button" disabled={change.isPending} onClick={() => change.mutate(() => setOverlayClickThroughDesktop(true))}>
          Click-through on
        </button>
        <button type="button" disabled={change.isPending} onClick={() => change.mutate(() => setOverlayClickThroughDesktop(false))}>
          Click-through off
        </button>
      </div>
      <p className="meta">
        {isDesktopShell()
          ? "Desktop shell detected: window commands are applied natively."
          : "Browser mode: preferences are saved, but the native window only moves inside the desktop shell."}
      </p>
      <form onSubmit={move} className="row">
        <label>
          X
          <input value={x} onChange={(event) => setX(event.target.value)} inputMode="numeric" />
        </label>
        <label>
          Y
          <input value={y} onChange={(event) => setY(event.target.value)} inputMode="numeric" />
        </label>
        <button type="submit" disabled={change.isPending}>Move</button>
      </form>
      <form onSubmit={setOpacity} className="row">
        <label>
          Opacity
          <input value={opacity} onChange={(event) => setOpacityState(event.target.value)} inputMode="decimal" />
        </label>
        <button type="submit" disabled={change.isPending}>Set opacity</button>
      </form>
      <form onSubmit={follow} className="row">
        <input value={sessionId} onChange={(event) => setSessionId(event.target.value)} placeholder="Active session id" />
        <button type="submit" disabled={sessionId.trim().length === 0}>Follow session</button>
      </form>
      {overlay.data?.active_session_id && (
        <div className="row">
          <button type="button" onClick={() => void download("markdown")}>Download Markdown</button>
          <button type="button" onClick={() => void download("json")}>Download JSON</button>
          {exportUrl && (
            <a href={exportUrl} download={exportName}>Save {exportName}</a>
          )}
        </div>
      )}
      {view.data?.items.length === 0 && overlay.data?.state !== "hidden" && (
        <p className="muted">No visible items for the active session.</p>
      )}
      {view.data?.items.map((item) => (
        <p key={item.id}>{item.speaker}: {item.text}</p>
      ))}
      {error && <p className="error">{error}</p>}
    </section>
  );
}
