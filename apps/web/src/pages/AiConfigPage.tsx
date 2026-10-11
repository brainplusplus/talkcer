import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { FormEvent, useState } from "react";
import {
  getAiConfigurations,
  getProviders,
  saveAiConfiguration,
  testProvider,
  type Capability,
  type ProviderConfig,
  type ProviderDescriptor,
} from "../api";

const EMPTY: Record<Capability, ProviderConfig> = {
  stt: { kind: "embedded", provider: "embedded-stt", model: "local" },
  thinker: {
    kind: "cloud",
    provider: "cloud-thinker",
    endpoint: "https://llm.example",
    model: "talkcer-thinker",
    credentials_ref: "THINKER_KEY",
  },
  tts: {
    kind: "self_hosted",
    provider: "self-hosted-tts",
    endpoint: "http://127.0.0.1:5002",
    model: "talkcer-voice",
  },
};

export function AiConfigPage() {
  const queryClient = useQueryClient();
  const configurations = useQuery({ queryKey: ["ai-config"], queryFn: getAiConfigurations });
  const providers = useQuery({ queryKey: ["ai-providers"], queryFn: getProviders });
  const [name, setName] = useState("Default");
  const [slots, setSlots] = useState(EMPTY);
  const [message, setMessage] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const save = useMutation({
    mutationFn: () => saveAiConfiguration({ name, ...slots }),
    onSuccess: async (saved) => {
      setError(null);
      setMessage(`Saved ${saved.name}.`);
      await queryClient.invalidateQueries({ queryKey: ["ai-config"] });
    },
    onError: (cause: Error) => setError(cause.message),
  });

  const test = useMutation({
    mutationFn: ({ id, capability }: { id: string; capability: Capability }) => testProvider(id, capability),
    onSuccess: (result) => setMessage(result.message),
    onError: (cause: Error) => setError(cause.message),
  });

  function update(capability: Capability, patch: Partial<ProviderConfig>) {
    setSlots((current) => ({ ...current, [capability]: { ...current[capability], ...patch } }));
  }

  function onSave(event: FormEvent) {
    event.preventDefault();
    save.mutate();
  }

  return (
    <section className="stack">
      <h2>AI configuration</h2>
      {configurations.isLoading && <p className="muted">Loading configuration…</p>}
      {configurations.data && configurations.data.length === 0 && <p className="muted">No configuration saved yet.</p>}
      {configurations.data?.map((item) => (
        <article key={item.id} className="card">
          <strong>{item.name}</strong>
          <p className="meta">
            STT {item.stt.provider} · Thinker {item.thinker.provider} · TTS {item.tts.provider}
          </p>
          <div className="row">
            {(["stt", "thinker", "tts"] as Capability[]).map((capability) => (
              <button key={capability} type="button" onClick={() => test.mutate({ id: item.id, capability })}>
                Test {capability}
              </button>
            ))}
          </div>
        </article>
      ))}
      <form onSubmit={onSave} className="stack">
        <label>
          Name
          <input value={name} onChange={(event) => setName(event.target.value)} />
        </label>
        {(["stt", "thinker", "tts"] as Capability[]).map((capability) => (
          <fieldset key={capability}>
            <legend>{capability}</legend>
            <label>
              Provider
              <select
                value={slots[capability].provider}
                onChange={(event) => {
                  const selected = providers.data?.find((item) => item.provider === event.target.value);
                  update(capability, { provider: event.target.value, kind: selected?.kind ?? slots[capability].kind });
                }}
              >
                {providers.data
                  ?.filter((item) => item.capability === capability)
                  .map((item) => (
                    <option key={item.provider} value={item.provider}>
                      {item.provider}
                    </option>
                  ))}
              </select>
            </label>
            <p className="meta">{capabilityText(providers.data?.find((item) => item.provider === slots[capability].provider))}</p>
            <label>
              Kind
              <input value={slots[capability].kind} readOnly />
            </label>
            <label>
              Model
              <input value={slots[capability].model} onChange={(event) => update(capability, { model: event.target.value })} />
            </label>
            {slots[capability].kind !== "embedded" && (
              <label>
                Endpoint
                <input
                  value={slots[capability].endpoint ?? ""}
                  onChange={(event) => update(capability, { endpoint: event.target.value })}
                />
              </label>
            )}
            {slots[capability].kind === "cloud" && (
              <label>
                Credential reference
                <input
                  value={slots[capability].credentials_ref ?? ""}
                  onChange={(event) => update(capability, { credentials_ref: event.target.value })}
                />
              </label>
            )}
          </fieldset>
        ))}
        <button type="submit" disabled={save.isPending}>
          {save.isPending ? "Saving…" : "Save configuration"}
        </button>
      </form>
      {message && <p className="meta">{message}</p>}
      {error && <p className="error">{error}</p>}
    </section>
  );
}

function capabilityText(provider: ProviderDescriptor | undefined): string {
  if (!provider) {
    return "Capability details load with the provider list.";
  }
  const features = [
    provider.capabilities.supports_streaming ? "streaming" : "complete",
    provider.capabilities.supports_partial_transcript ? "partial transcript" : "no partial transcript",
  ];
  const languages = provider.capabilities.supported_languages.join(", ") || "no languages";
  return `${features.join(" · ")} · ${languages}`;
}
