import { useState } from "react";
import { captureScreen, sendScreen } from "../api";

export function ScreenControls({ sessionId, onChange }: { sessionId: string; onChange: () => Promise<void> }) {
  const [direct, setDirect] = useState(true);
  const [note, setNote] = useState("");
  const [staged, setStaged] = useState(0);
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function capture() {
    setError(null);
    setPending(true);
    try {
      await captureScreen(sessionId, direct);
      if (!direct) {
        setStaged((count) => count + 1);
      }
      await onChange();
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : "Screen capture failed.");
    } finally {
      setPending(false);
    }
  }

  async function send() {
    setError(null);
    setPending(true);
    try {
      await sendScreen(sessionId, note.trim());
      setStaged(0);
      setNote("");
      await onChange();
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : "Send failed.");
    } finally {
      setPending(false);
    }
  }

  return (
    <section className="stack">
      <h3>Screen</h3>
      <p className="meta">
        Captures the whole current screen, so a question shown in a screen share can be read. The
        answer follows this session's mode.
      </p>
      <label className="row">
        <input type="checkbox" checked={direct} onChange={(event) => setDirect(event.target.checked)} />
        Answer immediately
      </label>
      <button type="button" className="primary" disabled={pending} onClick={() => void capture()}>
        {direct ? "Capture and answer" : "Capture to context"}
      </button>
      {!direct && (
        <div className="row">
          <input
            value={note}
            placeholder={staged > 0 ? `${staged} capture${staged > 1 ? "s" : ""} waiting. Add a note.` : "Add a note"}
            onChange={(event) => setNote(event.target.value)}
          />
          <button type="button" disabled={pending || (staged === 0 && note.trim() === "")} onClick={() => void send()}>
            Send
          </button>
        </div>
      )}
      {error && <p className="error">{error}</p>}
    </section>
  );
}
