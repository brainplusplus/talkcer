import { useEffect, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { getOverlay, setOverlayOpacityDesktop } from "../api";

const THEME_KEY = "talkcer-theme";
const LANG_KEY = "talkcer-language";
const STYLE_KEY = "talkcer-overlay-style";
const DISGUISE_KEY = "talkcer-disguise";

type Theme = "system" | "light" | "dark";
type Style = "default" | "liquid-glass" | "modern";

const DISGUISES = [
  { id: "none", label: "None" },
  { id: "terminal", label: "Command Prompt" },
  { id: "settings", label: "Settings" },
  { id: "activity", label: "Task Manager" },
];

export function GeneralPage() {
  const queryClient = useQueryClient();
  const overlay = useQuery({ queryKey: ["overlay"], queryFn: getOverlay });
  const [theme, setTheme] = useState<Theme>(() => stored(THEME_KEY, ["system", "light", "dark"] as const, "system"));
  const [language, setLanguage] = useState(() => stored(LANG_KEY, ["en", "id"] as const, "en"));
  const [style, setStyle] = useState<Style>(() => stored(STYLE_KEY, ["default", "liquid-glass", "modern"] as const, "default"));
  const [disguise, setDisguise] = useState(() => stored(DISGUISE_KEY, DISGUISES.map((item) => item.id), "none"));

  useEffect(() => {
    document.documentElement.dataset.theme = theme;
    localStorage.setItem(THEME_KEY, theme);
  }, [theme]);

  function remember<T extends string>(key: string, value: string, apply: (value: T) => void) {
    localStorage.setItem(key, value);
    apply(value as T);
  }

  const opacity = useMutation({
    mutationFn: (value: number) => setOverlayOpacityDesktop(value),
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: ["overlay"] });
    },
  });

  const currentOpacity = overlay.data?.opacity ?? 0.95;

  return (
    <section className="page">
      <header className="page-head">
        <div>
          <h2>General</h2>
          <p className="lede">How the window looks and which language the app speaks.</p>
        </div>
      </header>
      <div className="sheet">
      <Row
        title="Theme"
        detail="Follows the system, or stays light or dark."
        control={
          <Choice
            value={theme}
            options={[
              ["system", "System"],
              ["light", "Light"],
              ["dark", "Dark"],
            ]}
            onChange={(value) => setTheme(value as Theme)}
          />
        }
      />

      <Row
        title="Language"
        detail="The language the app speaks. Answers follow the model."
        control={
          <Choice
            value={language}
            options={[
              ["en", "English"],
              ["id", "Indonesia"],
            ]}
            onChange={(value) => remember<"en" | "id">(LANG_KEY, value, setLanguage)}
          />
        }
      />

      <Row
        title="Overlay style"
        detail="How the answer window is drawn."
        control={
          <Choice
            value={style}
            options={[
              ["default", "Default"],
              ["liquid-glass", "Liquid Glass"],
              ["modern", "Modern"],
            ]}
            onChange={(value) => remember<Style>(STYLE_KEY, value, setStyle)}
          />
        }
      />

      <Row
        title="Overlay opacity"
        detail="Drag to see it change in the preview."
        control={<span className="meta">{currentOpacity.toFixed(2)}</span>}
      />
      <input
        type="range"
        min="0.2"
        max="1"
        step="0.05"
        value={currentOpacity}
        onChange={(event) => opacity.mutate(Number(event.target.value))}
      />
      <div className="preview" style={{ opacity: currentOpacity }} data-style={style}>
        <strong>What the screen shows</strong>
        <p>This is how an answer sits on top of your work.</p>
      </div>

      <Row
        title="Disguise the window as"
        detail="Used while undetectable is on, so a shared screen shows an ordinary window."
        control={
          <Choice
            value={disguise}
            options={DISGUISES.map((item) => [item.id, item.label])}
            onChange={(value) => remember(DISGUISE_KEY, value, setDisguise)}
          />
        }
      />
      {opacity.isError && <p className="error">{(opacity.error as Error).message}</p>}
      </div>
    </section>
  );
}

function Row({ title, detail, control }: { title: string; detail: string; control: React.ReactNode }) {
  return (
    <div className="settings-row">
      <div>
        <h3>{title}</h3>
        <p className="meta">{detail}</p>
      </div>
      {control}
    </div>
  );
}

function Choice({
  value,
  options,
  onChange,
}: {
  value: string;
  options: string[][];
  onChange: (value: string) => void;
}) {
  return (
    <div className="choice">
      {options.map(([id, text]) => (
        <button key={id} type="button" className={id === value ? "active" : ""} onClick={() => onChange(id)}>
          {text}
        </button>
      ))}
    </div>
  );
}

function stored<T extends string>(key: string, allowed: readonly T[], fallback: T): T {
  const value = localStorage.getItem(key);
  return allowed.includes(value as T) ? (value as T) : fallback;
}
