import { useState } from "react";
import { captureControl, captureDevices, injectCapture, type CaptureDevice } from "../api";

const ACTIONS = ["start", "pause", "resume", "stop", "disconnect", "recover"] as const;

export function CaptureControls({ sessionId, onChange }: { sessionId: string; onChange: () => Promise<void> }) {
  const [state, setState] = useState("idle");
  const [devices, setDevices] = useState<CaptureDevice[]>([]);
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function control(action: (typeof ACTIONS)[number]) {
    setError(null);
    setPending(true);
    try {
      const status = await captureControl(sessionId, action);
      setState(`${status.platform}: ${status.state}`);
      if (status.devices) {
        setDevices(status.devices);
      }
      await onChange();
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : "Capture failed.");
    } finally {
      setPending(false);
    }
  }

  async function checkDevices() {
    setError(null);
    setPending(true);
    try {
      const opened = await captureDevices(sessionId);
      setDevices(opened.devices);
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : "Device probe failed.");
    } finally {
      setPending(false);
    }
  }

  async function inject(channel: "microphone" | "system_audio") {
    setError(null);
    setPending(true);
    try {
      await injectCapture(sessionId, channel, channel === "microphone" ? "Local fixture." : "Remote fixture.");
      await onChange();
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : "Fixture failed.");
    } finally {
      setPending(false);
    }
  }

  return (
    <div className="stack">
      <p className="meta">Windows capture · {state}</p>
      <div className="row">
        {ACTIONS.map((action) => (
          <button key={action} type="button" disabled={pending} onClick={() => void control(action)}>
            {action}
          </button>
        ))}
        <button type="button" disabled={pending} onClick={() => void checkDevices()}>
          Check devices
        </button>
        <button type="button" disabled={pending} onClick={() => void inject("microphone")}>
          Local fixture
        </button>
        <button type="button" disabled={pending} onClick={() => void inject("system_audio")}>
          Remote fixture
        </button>
      </div>
      {devices.length > 0 && (
        <ul className="meta" data-testid="capture-devices">
          {devices.map((device) => (
            <li key={device.channel}>
              {device.channel}: {device.name} · {device.direction} · {device.state} · {device.sample_rate} Hz ·{" "}
              {device.channels} ch
            </li>
          ))}
        </ul>
      )}
      {error && <p className="error">{error}</p>}
    </div>
  );
}
