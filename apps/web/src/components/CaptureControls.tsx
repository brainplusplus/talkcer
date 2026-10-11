import { useState } from "react";
import { captureControl, injectCapture } from "../api";

const ACTIONS = ["start", "pause", "resume", "stop", "disconnect", "recover"] as const;

export function CaptureControls({ sessionId, onChange }: { sessionId: string; onChange: () => Promise<void> }) {
  const [state, setState] = useState("idle");
  const [error, setError] = useState<string | null>(null);

  async function control(action: (typeof ACTIONS)[number]) {
    setError(null);
    try {
      const status = await captureControl(sessionId, action);
      setState(`${status.platform}: ${status.state}`);
      await onChange();
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : "Capture failed.");
    }
  }

  async function inject(channel: "microphone" | "system_audio") {
    setError(null);
    try {
      await injectCapture(sessionId, channel, channel === "microphone" ? "Local fixture." : "Remote fixture.");
      await onChange();
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : "Fixture failed.");
    }
  }

  return (
    <div className="stack">
      <p className="meta">Windows capture · {state}</p>
      <div className="row">
        {ACTIONS.map((action) => (
          <button key={action} type="button" onClick={() => void control(action)}>
            {action}
          </button>
        ))}
        <button type="button" onClick={() => void inject("microphone")}>Local fixture</button>
        <button type="button" onClick={() => void inject("system_audio")}>Remote fixture</button>
      </div>
      {error && <p className="error">{error}</p>}
    </div>
  );
}
