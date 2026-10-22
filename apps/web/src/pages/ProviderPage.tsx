import { useEffect, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  discoverModels,
  modelDownloads,
  startModelDownload,
  getAiConfigurations,
  getProviders,
  getSelection,
  modelCatalogs,
  probeProvider,
  saveAiConfiguration,
  saveSelection,
  type Capability,
  type DownloadStatus,
  type ModelCatalog,
  type ModelOption,
  type ProviderConfig,
  type ProviderKind,
  type ProviderSelection,
} from "../api";

// A provider you run yourself. Same shape as Natively's 9router card: an address
// and a key, with models discovered from the instance rather than a fixed list.
const GATEWAYS = [
  { id: "ollama", name: "Ollama", endpoint: "http://127.0.0.1:11434/v1", note: "Local models on this computer." },
  { id: "9router", name: "9Router", endpoint: "http://127.0.0.1:20128/v1", note: "Self-hosted proxy. Discovers models from your instance." },
  { id: "litellm", name: "LiteLLM", endpoint: "http://127.0.0.1:4000/v1", note: "Proxy in front of many providers." },
];

const EMPTY_SELECTION: ProviderSelection = { capability: "thinker", offered: [], disabled: [], defaults: {} };

function offeredMap(selection: ProviderSelection): Record<string, boolean> {
  return Object.fromEntries(selection.offered.map((item) => [`${item.provider}:${item.model}`, true]));
}

export function ProviderPage({
  capability,
  title,
  blurb,
}: {
  capability: Capability;
  title: string;
  blurb: string;
}) {
  const queryClient = useQueryClient();
  const registry = useQuery({ queryKey: ["providers"], queryFn: getProviders });
  const catalogs = useQuery({ queryKey: ["models"], queryFn: modelCatalogs });
  const configs = useQuery({ queryKey: ["ai-config"], queryFn: getAiConfigurations });
  const downloads = useQuery({ queryKey: ["downloads"], queryFn: modelDownloads });
  const selections = useQuery({ queryKey: ["selection"], queryFn: getSelection });
  const saved = configs.data?.[0];
  const stored = selections.data?.find((item) => item.capability === capability) ?? {
    ...EMPTY_SELECTION,
    capability,
  };
  const [offered, setOffered] = useState<Record<string, boolean>>(offeredMap(stored));
  const [defaults, setDefaults] = useState<Record<string, string>>(stored.defaults);
  const [disabled, setDisabled] = useState<string[]>(stored.disabled);
  const [discovered, setDiscovered] = useState<Record<string, ModelOption[]>>({});
  const [message, setMessage] = useState("");

  useEffect(() => {
    const current = selections.data?.find((item) => item.capability === capability);
    if (!current) return;
    setOffered(offeredMap(current));
    setDefaults(current.defaults);
    setDisabled(current.disabled);
  }, [selections.data, capability]);

  const relevant = (registry.data ?? []).filter(
    (item) => item.capability === capability && !item.provider.startsWith("embedded") && item.provider !== "disabled-tts",
  );
  const lists = (catalogs.data ?? []).filter((catalog) => catalog.capability === capability);

  const persist = useMutation({
    mutationFn: saveSelection,
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: ["selection"] });
    },
  });

  function commit(next: { offered: Record<string, boolean>; defaults: Record<string, string>; disabled: string[] }) {
    setOffered(next.offered);
    setDefaults(next.defaults);
    setDisabled(next.disabled);
    persist.mutate({
      capability,
      offered: Object.entries(next.offered)
        .filter(([, ticked]) => ticked)
        .map(([key]) => {
          const split = key.indexOf(":");
          return { provider: key.slice(0, split), model: key.slice(split + 1) };
        }),
      disabled: next.disabled,
      defaults: next.defaults,
    });
  }

  function toggle(provider: string, model: string) {
    const key = `${provider}:${model}`;
    commit({ ...view(), offered: { ...offered, [key]: !offered[key] } });
  }

  function makeDefault(provider: string, model: string) {
    commit({ ...view(), defaults: { ...defaults, [provider]: model } });
  }

  // Off keeps the saved key and address, exactly as Natively's provider switch
  // does. Turning a provider back on does not ask for them again.
  function setEnabled(provider: string, enabled: boolean) {
    const next = enabled ? disabled.filter((item) => item !== provider) : [...new Set([...disabled, provider])];
    commit({ ...view(), disabled: next });
  }

  function view() {
    return { offered, defaults, disabled };
  }

  const save = useMutation({
    mutationFn: (slot: ProviderConfig) =>
      saveAiConfiguration({
        id: saved?.id,
        name: saved?.name ?? "Default",
        stt: capability === "stt" ? slot : saved?.stt ?? embedded("stt"),
        thinker: capability === "thinker" ? slot : saved?.thinker ?? embedded("thinker"),
        tts: capability === "tts" ? slot : saved?.tts ?? embedded("tts"),
      }),
    onSuccess: async () => {
      setMessage("Saved.");
      await queryClient.invalidateQueries({ queryKey: ["ai-config"] });
    },
  });

  return (
    <section className="page">
      <header className="page-head">
        <div>
          <h2>{title}</h2>
          <p className="lede">{blurb}</p>
        </div>
      </header>
      <div className="provider-grid">
      {capability === "thinker" && GATEWAYS.map((gateway) => {
        const catalog = lists.find((item) => item.provider === gateway.id);
        const known = new Set((catalog?.models ?? []).map((model) => model.id));
        const extra = (discovered[gateway.id] ?? []).filter((model) => !known.has(model.id));
        return (
          <GatewayCard
            key={gateway.id}
            gateway={gateway}
            models={[...(catalog?.models ?? []), ...extra]}
            defaultId={catalog?.default_model ?? ""}
            offered={offered}
            chosen={defaults[gateway.id]}
            enabled={!disabled.includes(gateway.id)}
            onEnabled={(enabled) => setEnabled(gateway.id, enabled)}
            onToggle={(model) => toggle(gateway.id, model)}
            onDefault={(model) => makeDefault(gateway.id, model)}
            onDiscovered={(models) => setDiscovered((current) => ({ ...current, [gateway.id]: models }))}
          />
        );
      })}
      {relevant.map((provider) => {
        const catalog = lists.find((item) => item.provider === provider.provider);
        return (
          <ProviderCard
            key={provider.provider}
            provider={provider.provider}
            kind={provider.kind}
            catalog={catalog}
            offered={offered}
            chosen={defaults[provider.provider]}
            enabled={!disabled.includes(provider.provider)}
            onEnabled={(enabled) => setEnabled(provider.provider, enabled)}
            onToggle={(model) => toggle(provider.provider, model)}
            onDefault={(model) => makeDefault(provider.provider, model)}
            downloads={downloads.data ?? []}
            onUse={(slot) => save.mutate(slot)}
          />
        );
      })}
      {save.isError && <p className="error">{(save.error as Error).message}</p>}
      {message && <p className="meta">{message}</p>}
      </div>
    </section>
  );
}

function ProviderCard({
  provider,
  kind,
  catalog,
  offered,
  chosen,
  enabled,
  onEnabled,
  onToggle,
  onDefault,
  downloads,
  onUse,
}: {
  provider: string;
  kind: ProviderKind;
  catalog: ModelCatalog | undefined;
  offered: Record<string, boolean>;
  chosen: string | undefined;
  enabled: boolean;
  onEnabled: (enabled: boolean) => void;
  onToggle: (model: string) => void;
  onDefault: (model: string) => void;
  downloads: DownloadStatus[];
  onUse: (slot: ProviderConfig) => void;
}) {
  const [endpoint, setEndpoint] = useState(endpointFor(provider, kind));
  const [credential, setCredential] = useState("");
  const [probe, setProbe] = useState<{ state: "idle" | "testing" | "ok" | "failed"; message: string }>({ state: "idle", message: "" });
  const models = catalog?.models ?? [];

  async function testProviderCard() {
    setProbe({ state: "testing", message: "" });
    try {
      const result = await probeProvider({
        capability: catalog?.capability ?? "thinker",
        provider,
        endpoint,
        model: chosen || catalog?.default_model,
        credentials_ref: credential || undefined,
      });
      setProbe({ state: result.ok ? "ok" : "failed", message: result.message });
    } catch (cause) {
      setProbe({ state: "failed", message: cause instanceof Error ? cause.message : "Test failed." });
    }
  }

  return (
    <div className="card stack">
      <div className="row">
        <h3>{label(provider)}</h3>
        <Switch
          on={enabled}
          label={`${enabled ? "Disable" : "Enable"} ${label(provider)}`}
          onChange={onEnabled}
        />
      </div>
      {kind !== "embedded" && (
        <label>
          Endpoint
          <input value={endpoint} onChange={(event) => setEndpoint(event.target.value)} />
        </label>
      )}
      {kind === "cloud" && (
        <label>
          API key name
          <input
            value={credential}
            placeholder="stored outside the app, as TALKCER_SECRET_<NAME>"
            onChange={(event) => setCredential(event.target.value)}
          />
        </label>
      )}
      {models.length > 0 && (
        <ModelList
          provider={provider}
          models={models}
          defaultId={catalog?.default_model ?? ""}
          offered={offered}
          chosen={chosen}
          downloads={downloads}
          onToggle={onToggle}
          onDefault={onDefault}
        />
      )}
      <div className="row">
        {kind !== "embedded" && (
          <button type="button" className="quiet" onClick={() => void testProviderCard()} disabled={probe.state === "testing"}>
            {probe.state === "testing" ? "Testing…" : probe.state === "ok" ? "Passed" : "Test"}
          </button>
        )}
        <button
          type="button"
          className="primary"
          onClick={() =>
            onUse({
            kind,
            provider,
            endpoint: kind === "embedded" ? undefined : endpoint,
            model: chosen || catalog?.default_model || "",
            credentials_ref: credential || undefined,
            consent: kind === "cloud",
          })
        }
        >
          Use {label(provider)}
        </button>
      </div>
      {kind !== "embedded" && probe.message && <p className={probe.state === "ok" ? "meta" : "error"}>{probe.message}</p>}
    </div>
  );
}

function GatewayCard({
  gateway,
  models,
  defaultId,
  offered,
  chosen,
  enabled,
  onEnabled,
  onToggle,
  onDefault,
  onDiscovered,
}: {
  gateway: (typeof GATEWAYS)[number];
  models: ModelOption[];
  defaultId: string;
  offered: Record<string, boolean>;
  chosen: string | undefined;
  enabled: boolean;
  onEnabled: (enabled: boolean) => void;
  onToggle: (model: string) => void;
  onDefault: (model: string) => void;
  onDiscovered: (models: ModelOption[]) => void;
}) {
  const [endpoint, setEndpoint] = useState(gateway.endpoint);
  const [key, setKey] = useState("");
  const [status, setStatus] = useState<"idle" | "loading" | "empty">("idle");
  const [failure, setFailure] = useState<string | null>(null);
  const [probe, setProbe] = useState<{ state: "idle" | "testing" | "ok" | "failed"; message: string }>({ state: "idle", message: "" });

  async function testGateway() {
    setProbe({ state: "testing", message: "" });
    try {
      const result = await probeProvider({ capability: "thinker", provider: gateway.id, endpoint, model: chosen || defaultId });
      setProbe({ state: result.ok ? "ok" : "failed", message: result.message });
    } catch (cause) {
      setProbe({ state: "failed", message: cause instanceof Error ? cause.message : "Test failed." });
    }
  }

  async function refresh() {
    setFailure(null);
    setStatus("loading");
    try {
      const found = await discoverModels(endpoint, key || undefined);
      onDiscovered(found);
      setStatus(found.length === 0 ? "empty" : "idle");
    } catch (cause) {
      onDiscovered([]);
      setStatus("idle");
      setFailure(cause instanceof Error ? cause.message : "Could not look for models.");
    }
  }

  return (
    <div className="card stack">
      <div className="row">
        <h3>{gateway.name}</h3>
        <Switch
          on={enabled}
          label={`${enabled ? "Disable" : "Enable"} ${gateway.name}`}
          onChange={onEnabled}
        />
      </div>
      <p className="meta">{gateway.note}</p>
      <label className="stack">
        Base URL
        <input value={endpoint} onChange={(event) => setEndpoint(event.target.value)} />
      </label>
      <label className="stack">
        API key
        <input
          type="password"
          value={key}
          placeholder="Only if your instance asks for one"
          onChange={(event) => setKey(event.target.value)}
        />
      </label>
      <div className="row">
        <button type="button" className="quiet" onClick={() => void testGateway()} disabled={probe.state === "testing"}>
          {probe.state === "testing" ? "Testing…" : probe.state === "ok" ? "Passed" : "Test"}
        </button>
        <button type="button" className="quiet" onClick={() => void refresh()} disabled={status === "loading"}>
          Refresh
        </button>
      </div>
      {probe.message && <p className={probe.state === "ok" ? "meta" : "error"}>{probe.message}</p>}
      {status === "loading" && <p className="meta">Looking for models…</p>}
      {status === "empty" && <p className="meta">No models found. Is {gateway.name} running at this address?</p>}
      {failure && <p className="error">{failure}</p>}
      {models.length > 0 && (
        <ModelList
          provider={gateway.id}
          models={models}
          defaultId={defaultId}
          offered={offered}
          chosen={chosen}
          downloads={[]}
          onToggle={onToggle}
          onDefault={onDefault}
        />
      )}
    </div>
  );
}

function Switch({
  on,
  label,
  onChange,
}: {
  on: boolean;
  label: string;
  onChange: (next: boolean) => void;
}) {
  return (
    <button
      type="button"
      role="switch"
      className="switch"
      data-on={String(on)}
      aria-checked={on}
      aria-label={label}
      title={on ? "Disable provider. Keeps the saved key and address." : "Enable provider"}
      onClick={() => onChange(!on)}
    >
      <span />
    </button>
  );
}

function ModelList({
  provider,
  models,
  defaultId,
  offered,
  chosen,
  downloads,
  onToggle,
  onDefault,
}: {
  provider: string;
  models: ModelOption[];
  defaultId: string;
  offered: Record<string, boolean>;
  chosen: string | undefined;
  downloads: DownloadStatus[];
  onToggle: (model: string) => void;
  onDefault: (model: string) => void;
}) {
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");
  const active = chosen || defaultId;
  const visible = models.filter((model) => model.label.toLowerCase().includes(query.toLowerCase()));
  const offeredCount = models.filter(
    (model) => offered[`${provider}:${model.id}`] ?? model.id === defaultId,
  ).length;

  return (
    <div className="model-list">
      <button type="button" className="quiet model-summary" aria-expanded={open} onClick={() => setOpen((value) => !value)}>
        <span>Models</span>
        <span className="meta">{offeredCount} of {models.length} · default {active}</span>
      </button>
      {open && (
        <div className="model-well">
          {models.length > 6 && (
            <input
              value={query}
              placeholder="Filter models"
              onChange={(event) => setQuery(event.target.value)}
            />
          )}
          {visible.map((model) => (
            <ModelRow
              key={model.id}
              provider={provider}
              model={model}
              offered={offered[`${provider}:${model.id}`] ?? model.id === defaultId}
              isDefault={active === model.id}
              download={downloads.find((item) => item.provider === provider && item.model === model.id)}
              onToggle={() => onToggle(model.id)}
              onDefault={() => onDefault(model.id)}
            />
          ))}
          {visible.length === 0 && <p className="meta">No model matches.</p>}
        </div>
      )}
    </div>
  );
}

function ModelRow({
  provider,
  model,
  offered,
  isDefault,
  download,
  onToggle,
  onDefault,
}: {
  provider: string;
  model: ModelOption;
  offered: boolean;
  isDefault: boolean;
  download: DownloadStatus | undefined;
  onToggle: () => void;
  onDefault: () => void;
}) {
  const queryClient = useQueryClient();
  const [busy, setBusy] = useState(false);
  const [failure, setFailure] = useState("");

  async function fetchModel() {
    if (!download) return;
    setBusy(true);
    setFailure("");
    try {
      await startModelDownload({ capability: download.capability, provider, model: model.id });
      await queryClient.invalidateQueries({ queryKey: ["downloads"] });
    } catch (cause) {
      setFailure(cause instanceof Error ? cause.message : "Download failed.");
    } finally {
      setBusy(false);
    }
  }

  const stateLabel = download?.state === "ready" ? "Ready" : download?.state === "partial" ? "Resume" : "Download";
  return (
    <div className="model-row">
      <input type="checkbox" checked={offered} onChange={onToggle} aria-label={`offer ${model.label}`} />
      <div>
        <strong>{model.label}</strong>
        <p className="meta">
          {model.local ? `${model.size_mb} MB` : "Runs remotely"}
          {model.english_only ? " · English only" : model.local ? " · Multilingual" : ""}
          {model.speed ? ` · ${speedLabel(model.speed)}` : ""}
          {model.accuracy ? ` · accuracy ${accuracyLabel(model.accuracy)}` : ""}
        </p>
      </div>
      <span className="model-actions">
        {model.local && <ModelTest capability={download?.capability ?? "stt"} provider={provider} model={model.id} />}
        {model.local && download && (
          <button type="button" className="quiet" onClick={() => void fetchModel()} disabled={busy || download.state === "ready"}>
            {busy ? "Downloading…" : stateLabel}
          </button>
        )}
        <button type="button" className={isDefault ? "" : "quiet"} onClick={onDefault}>
          {isDefault ? "Default" : "Set default"}
        </button>
      </span>
      {failure && <p className="error">{failure}</p>}
    </div>
  );
}

function ModelTest({ capability, provider, model }: { capability: Capability; provider: string; model: string }) {
  const [state, setState] = useState<"idle" | "testing" | "ok" | "failed">("idle");
  const [message, setMessage] = useState("");

  async function run() {
    setState("testing");
    setMessage("");
    try {
      const result = await probeProvider({ capability, provider, model });
      setState(result.ok ? "ok" : "failed");
      setMessage(result.message);
    } catch (cause) {
      setState("failed");
      setMessage(cause instanceof Error ? cause.message : "Test failed.");
    }
  }

  return (
    <>
      <button type="button" className="quiet" onClick={() => void run()} disabled={state === "testing"}>
        {state === "testing" ? "Testing…" : state === "ok" ? "Passed" : "Test"}
      </button>
      {message && (
        <span className={state === "ok" ? "meta" : "error"} role="status">
          {message}
        </span>
      )}
    </>
  );
}

const SPEED: Record<string, string> = {
  "very-fast": "Very fast",
  fast: "Fast",
  medium: "Medium",
  slow: "Slow",
};

const ACCURACY: Record<string, string> = {
  decent: "decent",
  good: "good",
  high: "high",
  "very-high": "very high",
};

function speedLabel(value: string): string {
  return SPEED[value] ?? value;
}

function accuracyLabel(value: string): string {
  return ACCURACY[value] ?? value;
}

function embedded(capability: Capability): ProviderConfig {
  const names: Record<Capability, string> = { stt: "embedded-stt", thinker: "embedded-thinker", tts: "embedded-tts" };
  return { kind: "embedded", provider: names[capability], model: "embedded" };
}

function endpointFor(provider: string, kind: ProviderKind): string {
  if (provider === "ollama") return "http://127.0.0.1:11434";
  if (provider === "openai" || provider === "stt-openai" || provider === "tts-openai") return "https://api.openai.com/v1";
  if (provider === "groq") return "https://api.groq.com/openai/v1";
  if (provider === "deepseek") return "https://api.deepseek.com";
  if (provider === "deepgram") return "https://api.deepgram.com";
  return kind === "self_hosted" ? "http://127.0.0.1:8000/v1" : "";
}

function label(provider: string): string {
  const names: Record<string, string> = {
    ollama: "Ollama",
    openai: "OpenAI",
    groq: "Groq",
    deepseek: "DeepSeek",
    "thinker-custom": "Custom (OpenAI compatible)",
    whisper: "Whisper",
    deepgram: "Deepgram",
    "stt-openai": "OpenAI",
    piper: "Piper",
    "tts-openai": "OpenAI",
    "tts-custom": "Custom",
  };
  return names[provider] ?? provider;
}


