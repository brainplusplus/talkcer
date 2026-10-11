import { useState } from "react";
import { sendVoiceAudio, speakText, updateSession, type Session, type TimelineItem } from "../api";

export function VoiceControls({
  session,
  items,
  onChange,
}: {
  session: Session;
  items: TimelineItem[];
  onChange: () => Promise<void>;
}) {
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const ttsEnabled = session.configuration.tts === "enabled";
  const latestSpeech = [...items].reverse().find((item) => item.speaker === "ai" && item.text.length > 0);

  async function sendScript(source: "browser_mic" | "mock_audio", text: string) {
    setBusy(true);
    setError(null);
    try {
      const partial = await sendVoiceAudio(session.id, { audioSource: source, text: text.slice(0, 3), isFinal: false });
      await sendVoiceAudio(session.id, {
        utteranceId: partial.item.id,
        audioSource: source,
        text,
        isFinal: true,
      });
      await onChange();
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : "Voice request failed.");
    } finally {
      setBusy(false);
    }
  }

  async function useBrowserMic() {
    if (!navigator.mediaDevices?.getUserMedia) {
      await sendScript("mock_audio", "Browser microphone is unavailable, so this is mock audio.");
      return;
    }
    try {
      const stream = await navigator.mediaDevices.getUserMedia({ audio: true });
      stream.getTracks().forEach((track) => track.stop());
      await sendScript("browser_mic", "Browser microphone captured a mock transcript.");
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : "Microphone permission was denied.");
    }
  }

  async function toggleSpeech() {
    await updateSession(session.id, {
      configuration: { ...session.configuration, tts: ttsEnabled ? "disabled" : "enabled" },
    });
    await onChange();
  }

  async function play() {
    if (!latestSpeech) {
      return;
    }
    const result = await speakText(session.id, latestSpeech.text);
    if (!result.enabled || !result.audio_base64) {
      setError("Turn TTS on before playing a response.");
      return;
    }
    const audio = new Audio(`data:audio/wav;base64,${result.audio_base64}`);
    await audio.play();
  }

  return (
    <div className="stack">
      <div className="row">
        <button type="button" onClick={() => void useBrowserMic()} disabled={busy}>
          Browser mic
        </button>
        <button type="button" onClick={() => void sendScript("mock_audio", "This is a mock audio transcript.")} disabled={busy}>
          Mock audio
        </button>
        <button type="button" onClick={() => void toggleSpeech()}>
          TTS {ttsEnabled ? "on" : "off"}
        </button>
        <button type="button" onClick={() => void play()} disabled={!latestSpeech}>
          Play response
        </button>
      </div>
      {error && <p className="error">{error}</p>}
    </div>
  );
}
