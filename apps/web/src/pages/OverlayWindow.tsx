import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect, useMemo } from "react";
import {
  getOverlay,
  isDesktopShell,
  overlayQuickAction,
  overlayView,
  setOverlayClickThroughDesktop,
  setOverlayOpacityDesktop,
} from "../api";
import type { TimelineItem } from "../timeline";

/**
 * The compact always-on-top overlay window.
 *
 * This page is loaded into the Tauri `overlay` window. It renders only the
 * active session and offers the quick actions from the UI spec. It never
 * captures audio and never calls a model directly: the desktop shell forwards
 * quick actions to the core API.
 */
export function OverlayWindow() {
  const queryClient = useQueryClient();
  const overlay = useQuery({ queryKey: ["overlay"], queryFn: getOverlay });
  const view = useQuery({
    queryKey: ["overlay-view"],
    queryFn: overlayView,
    enabled: overlay.data?.state !== "hidden",
    refetchInterval: 1500,
  });

  const change = useMutation({
    mutationFn: (action: () => Promise<unknown>) => action(),
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: ["overlay"] });
      await queryClient.invalidateQueries({ queryKey: ["overlay-view"] });
    },
  });

  const items = view.data?.items ?? [];
  const latestRemote = useMemo(
    () => [...items].reverse().find((item) => item.speaker === "remote" && item.is_final),
    [items],
  );

  // The overlay must never be visible with nothing to show.
  const visible = overlay.data?.state !== "hidden";

  useEffect(() => {
    document.body.dataset.overlay = "true";
    return () => {
      delete document.body.dataset.overlay;
    };
  }, []);

  return (
    <div className="overlay-window" style={{ opacity: overlay.data?.opacity ?? 0.95 }}>
      <header className="overlay-head" data-tauri-drag-region>
        <span className="overlay-title">Talkcer Overlay</span>
        <span className="overlay-live">
          {visible ? (latestRemote ? "Live" : "Idle") : "Hidden"}
        </span>
        <button
          type="button"
          aria-label="Hide overlay"
          onClick={() => change.mutate(() => overlayQuickAction("hide"))}
        >
          ×
        </button>
      </header>
      <ol className="overlay-body">
        {items.length === 0 && <li className="muted">Waiting for the active session…</li>}
        {items.map((item) => (
          <li key={item.utterance_id ?? item.id} className={`overlay-line ${item.speaker}`}>
            <span className="overlay-speaker">{speakerLabel(item.speaker)}</span>
            <p>{item.text}</p>
          </li>
        ))}
      </ol>
      <footer className="overlay-actions">
        <button
          type="button"
          disabled={!latestRemote || change.isPending}
          onClick={() => change.mutate(() => overlayQuickAction("explain"))}
        >
          Explain
        </button>
        <button
          type="button"
          disabled={!latestRemote || change.isPending}
          onClick={() => change.mutate(() => overlayQuickAction("translate"))}
        >
          Translate
        </button>
        <button
          type="button"
          disabled={!latestRemote || change.isPending}
          onClick={() => change.mutate(() => overlayQuickAction("suggest"))}
        >
          Suggest
        </button>
        <button type="button" onClick={() => change.mutate(() => overlayQuickAction("hide"))}>
          Hide
        </button>
      </footer>
      {isDesktopShell() && overlay.data && (
        <div className="overlay-desktop">
          <label>
            Opacity
            <input
              type="range"
              min="0.2"
              max="1"
              step="0.05"
              defaultValue={overlay.data.opacity}
              onChange={(event) =>
                change.mutate(() => setOverlayOpacityDesktop(Number.parseFloat(event.target.value)))
              }
            />
          </label>
          <button
            type="button"
            onClick={() =>
              change.mutate(() =>
                setOverlayClickThroughDesktop(overlay.data?.state !== "click_through"),
              )
            }
          >
            {overlay.data.state === "click_through" ? "Click-through on" : "Click-through off"}
          </button>
        </div>
      )}
    </div>
  );
}

function speakerLabel(speaker: TimelineItem["speaker"]): string {
  switch (speaker) {
    case "remote":
      return "Interviewer";
    case "local_user":
      return "Saya";
    case "ai":
      return "AI";
    default:
      return "System";
  }
}
