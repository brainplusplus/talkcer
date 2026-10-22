import { useEffect, useId, useRef, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  audioEndpoints,
  audioSelection,
  modelCatalogs,
  modelDownloads,
  saveAudioSelection,
  type AudioSelection,
  type ModelOption,
} from "../api";

/**
 * Chooses which downloaded speech model reads each channel, and which physical
 * device capture opens for the microphone and for system audio.
 *
 * System audio is captured from a playback device in loopback, so the second
 * picker lists playback devices, not recording devices. Both the devices and
 * the models are saved on the server, because capture runs there.
 */
export function AudioPage() {
  const catalogs = useQuery({ queryKey: ["catalogs"], queryFn: modelCatalogs });
  const downloads = useQuery({ queryKey: ["downloads"], queryFn: modelDownloads });
  const ready = readyModels(catalogs.data, downloads.data);
  const loading = catalogs.isLoading || downloads.isLoading;
  const failed = catalogs.isError || downloads.isError;

  return (
    <section className="page">
      <header className="page-head">
        <div>
          <h2>Audio</h2>
          <p className="lede">
            Pick the microphone and the playback device Talkcer listens to, and the speech model
            that reads each one. Only models that have finished downloading are listed.
          </p>
        </div>
      </header>
      <div className="sheet">
        <DevicePicker />
        <ModelPicker
          title="Microphone model"
          detail="Reads your own voice."
          field="microphone_model"
          models={ready}
          loading={loading}
          failed={failed}
        />
        <ModelPicker
          title="System audio model"
          detail="Reads the other person, from the playback device above."
          field="system_model"
          models={ready}
          loading={loading}
          failed={failed}
        />
        <MicLevel />
      </div>
    </section>
  );
}

function readyModels(
  catalogs: { capability: string; provider: string; models: ModelOption[] }[] | undefined,
  downloads: { model: string; provider: string; state: string }[] | undefined,
): ModelOption[] {
  const finished = new Set(
    (downloads ?? [])
      .filter((item) => item.state === "ready" && item.provider === "whisper")
      .map((item) => item.model),
  );
  return (catalogs ?? [])
    .filter((catalog) => catalog.capability === "stt" && catalog.provider === "whisper")
    .flatMap((catalog) => catalog.models)
    .filter((model) => finished.has(model.id));
}

/**
 * One saved audio selection plus a mutation that writes it back.
 *
 * The cache is updated optimistically so a select does not flick back to the
 * previous value while the request is in flight.
 */
function useAudioSelection() {
  const queryClient = useQueryClient();
  const query = useQuery({ queryKey: ["audio-selection"], queryFn: audioSelection });
  const save = useMutation({
    mutationFn: saveAudioSelection,
    onMutate: async (next: AudioSelection) => {
      await queryClient.cancelQueries({ queryKey: ["audio-selection"] });
      const previous = queryClient.getQueryData<AudioSelection>(["audio-selection"]);
      queryClient.setQueryData(["audio-selection"], next);
      return { previous };
    },
    onError: (_error, _next, context) => {
      if (context?.previous) {
        queryClient.setQueryData(["audio-selection"], context.previous);
      }
    },
    onSettled: async () => {
      await queryClient.invalidateQueries({ queryKey: ["audio-selection"] });
    },
  });
  const current: AudioSelection = query.data ?? {
    microphone: "",
    playback: "",
    microphone_model: "",
    system_model: "",
  };
  return { current, save, query };
}

function DevicePicker() {
  const { current, save, query } = useAudioSelection();
  const devices = useQuery({ queryKey: ["audio-devices"], queryFn: audioEndpoints });
  const microphones = (devices.data?.devices ?? []).filter((device) => device.direction === "capture");
  const playback = (devices.data?.devices ?? []).filter((device) => device.direction === "render");

  return (
    <>
      <Picker
        title="Microphone"
        detail="The recording device Talkcer reads your voice from."
        devices={microphones}
        value={current.microphone}
        onChange={(microphone) => save.mutate({ ...current, microphone })}
      />
      <Picker
        title="System audio"
        detail="The playback device whose sound Talkcer transcribes."
        devices={playback}
        value={current.playback}
        onChange={(next) => save.mutate({ ...current, playback: next })}
      />
      {devices.isError && <p className="error">Could not list the audio devices. Check that the local API is running.</p>}
      {query.isError && <p className="error">Could not load the saved audio choice.</p>}
      {save.isError && <p className="error">{(save.error as Error).message}</p>}
    </>
  );
}

function Picker({
  title,
  detail,
  devices,
  value,
  onChange,
}: {
  title: string;
  detail: string;
  devices: { id: string; name: string }[];
  value: string;
  onChange: (value: string) => void;
}) {
  const id = useId();
  return (
    <div className="settings-row">
      <div>
        <label htmlFor={id}>
          <h3>{title}</h3>
        </label>
        <p className="meta">{detail}</p>
      </div>
      <select id={id} value={value} onChange={(event) => onChange(event.target.value)}>
        <option value="">System default</option>
        {devices.map((device) => (
          <option key={device.id} value={device.id}>
            {device.name}
          </option>
        ))}
      </select>
    </div>
  );
}

function ModelPicker({
  title,
  detail,
  field,
  models,
  loading,
  failed,
}: {
  title: string;
  detail: string;
  field: "microphone_model" | "system_model";
  models: ModelOption[];
  loading: boolean;
  failed: boolean;
}) {
  const id = useId();
  const { current, save } = useAudioSelection();
  const value = current[field];
  const empty = models.length === 0;

  return (
    <div className="settings-row">
      <div>
        <label htmlFor={id}>
          <h3>{title}</h3>
        </label>
        <p className="meta">{detail}</p>
      </div>
      <select
        id={id}
        value={value}
        onChange={(event) => save.mutate({ ...current, [field]: event.target.value })}
        disabled={loading}
      >
        <option value="">
          {loading
            ? "Loading…"
            : failed
              ? "Could not load models"
              : empty
                ? "No model downloaded yet"
                : "Same as the session"}
        </option>
        {models.map((model) => (
          <option key={model.id} value={`whisper:${model.id}`}>
            {model.label}
          </option>
        ))}
      </select>
    </div>
  );
}

/**
 * Drop the suffixes the server adds so a label matches the browser's own.
 *
 * The server labels the default endpoint with `(default)` and repeated names
 * with a counter, while `enumerateDevices` returns the bare friendly name.
 */
function normalizeDeviceName(name: string): string {
  return name
    .replace(/ \(default\)$/, "")
    .replace(/ \(\d+\)$/, "")
    .trim()
    .toLowerCase();
}

/**
 * Map a server device onto the browser's own device id.
 *
 * The server names a WASAPI endpoint (`{0.0.1.…}`), but `getUserMedia` only
 * accepts the opaque ids `enumerateDevices` returns, so the two are matched by
 * friendly name.
 */
function matchBrowserDevice(devices: MediaDeviceInfo[], serverName: string): string | undefined {
  const wanted = normalizeDeviceName(serverName);
  return devices
    .filter((device) => device.kind === "audioinput" && device.deviceId)
    .find((device) => normalizeDeviceName(device.label) === wanted)?.deviceId;
}

/**
 * Reject when a browser prompt never answers.
 *
 * `getUserMedia` stays pending while its permission prompt is on screen, which
 * would otherwise leave the button spinning with no explanation.
 */
function withTimeout<T>(promise: Promise<T>, message: string): Promise<T> {
  return Promise.race([
    promise,
    new Promise<never>((_, reject) => {
      window.setTimeout(() => reject(new Error(message)), 8000);
    }),
  ]);
}

/**
 * Turn a `getUserMedia` failure into a sentence a person can act on.
 *
 * Chromium reports several of these with an empty `message`, so the error name
 * is the only reliable part.
 */
function describeMicrophoneError(cause: unknown): string {
  const name = cause instanceof DOMException ? cause.name : "";
  switch (name) {
    case "NotAllowedError":
    case "PermissionDeniedError":
      return "The browser blocked microphone access. Allow the microphone for this site, then try again.";
    case "NotFoundError":
    case "DevicesNotFoundError":
      return "No microphone matched the saved device. Pick another one above.";
    case "NotReadableError":
    case "TrackStartError":
      return "The device is in use by another app. Close that app, then try again.";
    case "OverconstrainedError":
      return "The saved microphone is no longer connected. Pick another one above.";
    default:
      break;
  }
  return cause instanceof Error && cause.message.trim()
    ? cause.message.trim()
    : "The microphone could not be opened.";
}

function MicLevel() {
  const [starting, setStarting] = useState(false);
  const [listening, setListening] = useState(false);
  const [error, setError] = useState("");
  const stop = useRef<(() => void) | null>(null);
  const bar = useRef<HTMLSpanElement | null>(null);
  const { current } = useAudioSelection();
  const devices = useQuery({ queryKey: ["audio-devices"], queryFn: audioEndpoints });
  const chosenName =
    (devices.data?.devices ?? []).find((device) => device.id === current.microphone)?.name ?? "";

  useEffect(() => () => stop.current?.(), []);

  async function toggle() {
    if (listening) {
      stop.current?.();
      stop.current = null;
      setListening(false);
      if (bar.current) {
        bar.current.style.width = "0%";
      }
      return;
    }
    if (starting) {
      return;
    }
    if (!navigator.mediaDevices?.getUserMedia) {
      setError("This browser cannot open a microphone.");
      return;
    }
    setStarting(true);
    let opened: MediaStream | null = null;
    try {
      // Browser device labels are hidden until permission is granted, so open
      // the default first, then switch to the chosen device by name.
      let stream = await withTimeout(
        navigator.mediaDevices.getUserMedia({ audio: true }),
        "The browser did not answer the microphone request. Check the permission prompt.",
      );
      opened = stream;
      if (chosenName) {
        const browserDevices = await navigator.mediaDevices.enumerateDevices();
        const wanted = matchBrowserDevice(browserDevices, chosenName);
        const active = stream.getAudioTracks()[0]?.getSettings().deviceId;
        if (wanted && wanted !== active) {
          stream.getTracks().forEach((track) => track.stop());
          stream = await withTimeout(
            navigator.mediaDevices.getUserMedia({ audio: { deviceId: { exact: wanted } } }),
            "The chosen microphone did not open. Check the permission prompt.",
          );
          opened = stream;
        }
      }
      const context = new AudioContext();
      const analyser = context.createAnalyser();
      analyser.fftSize = 1024;
      context.createMediaStreamSource(stream).connect(analyser);
      const samples = new Uint8Array(analyser.fftSize);
      let frame = 0;
      // Write the bar width directly. Driving this through state would render
      // the whole page sixty times a second for one moving rectangle.
      const tick = () => {
        analyser.getByteTimeDomainData(samples);
        let sum = 0;
        for (const sample of samples) {
          const centered = (sample - 128) / 128;
          sum += centered * centered;
        }
        const level = Math.min(1, Math.sqrt(sum / samples.length) * 4);
        if (bar.current) {
          bar.current.style.width = `${Math.round(level * 100)}%`;
        }
        frame = requestAnimationFrame(tick);
      };
      frame = requestAnimationFrame(tick);
      stop.current = () => {
        cancelAnimationFrame(frame);
        stream.getTracks().forEach((track) => track.stop());
        void context.close();
      };
      setError("");
      setListening(true);
    } catch (cause) {
      // A failed switch would otherwise leave the default microphone open.
      opened?.getTracks().forEach((track) => track.stop());
      setError(describeMicrophoneError(cause));
    } finally {
      setStarting(false);
    }
  }

  return (
    <div className="settings-row">
      <div>
        <h3>Microphone check</h3>
        <p className="meta">Speak and the bar should move. Nothing is recorded.</p>
        <div className="level" aria-hidden="true">
          <span ref={bar} />
        </div>
        {error && <p className="error">{error}</p>}
      </div>
      <button type="button" onClick={() => void toggle()} disabled={starting}>
        {starting ? "Starting…" : listening ? "Stop" : "Listen"}
      </button>
    </div>
  );
}
