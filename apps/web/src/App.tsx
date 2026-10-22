import { useQuery } from "@tanstack/react-query";
import { NavLink, Navigate, Route, Routes, useLocation } from "react-router-dom";
import { currentSession, getHealth, type SessionMode } from "./api";
import { AboutPage } from "./pages/AboutPage";
import { AudioPage } from "./pages/AudioPage";
import { GeneralPage } from "./pages/GeneralPage";
import { PlaceholderPage } from "./pages/PlaceholderPage";
import { ProviderPage } from "./pages/ProviderPage";
import { NewSessionPage } from "./pages/NewSessionPage";
import { ProfilePage } from "./pages/ProfilePage";
import { OverlayPage } from "./pages/OverlayPage";
import { OverlayWindow } from "./pages/OverlayWindow";
import { PrivacyPage } from "./pages/PrivacyPage";
import { RemotePage } from "./pages/RemotePage";
import { SessionDetailPage } from "./pages/SessionDetailPage";
import { SessionListPage } from "./pages/SessionListPage";

function CurrentSpace() {
  const current = useQuery({
    queryKey: ["current-session"],
    queryFn: async () => {
      try {
        return await currentSession();
      } catch {
        return { session: null, paused: false };
      }
    },
    refetchInterval: 5000,
  });
  const session = current.data?.session;
  return (
    <>
      <p className="nav-label">Current live space</p>
      {session ? (
        <NavLink to={`/${MODE_PATH[session.mode]}/${session.id}`}>{session.title}</NavLink>
      ) : (
        <p className="meta">No session running</p>
      )}
    </>
  );
}

const MODE_PATH: Record<SessionMode, string> = {
  live_interview: "live",
  mock_interview: "mock",
  learning: "learning",
};

const SETTINGS: Array<{ to: string; label: string }> = [
  { to: "/settings/profile", label: "Profile" },
  { to: "/settings/general", label: "General" },
  { to: "/settings/llm", label: "LLM Provider" },
  { to: "/settings/stt", label: "STT Provider" },
  { to: "/settings/tts", label: "TTS Provider" },
  { to: "/settings/embedding", label: "Embedding Provider" },
  { to: "/settings/skills", label: "Skills" },
  { to: "/settings/mcp", label: "MCP" },
  { to: "/settings/audio", label: "Audio" },
  { to: "/settings/keybinds", label: "Keybinds" },
  { to: "/settings/privacy", label: "Privacy" },
  { to: "/overlay", label: "Overlay" },
];

const NAV: Array<{ to: string; label: string; mode: SessionMode }> = [
  { to: "/live", label: "Live Interview", mode: "live_interview" },
  { to: "/mock", label: "Mock Interview", mode: "mock_interview" },
  { to: "/learning", label: "Learning", mode: "learning" },
];

export function App() {
  const health = useQuery({ queryKey: ["health"], queryFn: getHealth });
  const location = useLocation();

  // The overlay window loads its own compact page and no sidebar.
  if (location.pathname === "/overlay-window") {
    return <OverlayWindow />;
  }

  return (
    <div className="app">
      <aside className="sidebar">
        <h1>Talkcer</h1>
        <nav>
          <CurrentSpace />
          <p className="nav-label">Spaces</p>
          {NAV.map((item) => (
            <NavLink key={item.to} to={item.to}>
              {item.label}
            </NavLink>
          ))}
          <p className="nav-label">Settings</p>
          {SETTINGS.map((item) => (
            <NavLink key={item.to} to={item.to}>
              {item.label}
            </NavLink>
          ))}
          <p className="nav-label">About</p>
          <NavLink to="/about">About</NavLink>
        </nav>
        <p className="meta">
          {health.isLoading && "Connecting…"}
          {health.isError && "API unavailable"}
          {health.data && `${health.data.app} ${health.data.version}`}
        </p>
      </aside>
      <main className="main">
        <Routes>
          <Route path="/" element={<Navigate to="/live" replace />} />
          <Route path="/settings" element={<Navigate to="/settings/general" replace />} />
          <Route path="/settings/profile" element={<ProfilePage />} />
          <Route path="/settings/general" element={<GeneralPage />} />
          <Route path="/settings/llm" element={<ProviderPage capability="thinker" title="LLM Provider" blurb="The language model that answers. Local models come from Ollama; a custom provider speaks the OpenAI chat API." />} />
          <Route path="/settings/stt" element={<ProviderPage capability="stt" title="STT Provider" blurb="Turns speech into text. Whisper runs locally after its model is downloaded; Deepgram and OpenAI run in the cloud." />} />
          <Route path="/settings/tts" element={<ProviderPage capability="tts" title="TTS Provider" blurb="Speaks answers in mock and learning. Piper runs locally after its voice is downloaded." />} />
          <Route path="/settings/embedding" element={<PlaceholderPage title="Embedding Provider" note="Prepared, not built yet. It will list local Ollama embedding models and cloud or custom OpenAI-compatible embedders, the same way the other providers do." />} />
          <Route path="/settings/skills" element={<PlaceholderPage title="Skills" note="Prepared, not built yet. A skill is an instruction file the model reads for one kind of task. Saving them comes before the model can use them." />} />
          <Route path="/settings/mcp" element={<PlaceholderPage title="MCP" note="Prepared, not built yet. An MCP server is a tool process the model can call. This page will store how to start it before any session uses it." />} />
          <Route path="/settings/audio" element={<AudioPage />} />
          <Route path="/settings/keybinds" element={<PlaceholderPage title="Keybinds" note="The shortcuts are fixed for now: Ctrl+1 shows the app, Ctrl+2 toggles undetectable, Ctrl+3 toggles the overlay, Ctrl+4 opens settings, Ctrl+5 opens the running session. Recording your own comes later." />} />
          <Route path="/about" element={<AboutPage />} />
          <Route path="/settings/ai" element={<Navigate to="/settings/llm" replace />} />
          <Route path="/settings/privacy" element={<PrivacyPage />} />
          <Route path="/remote" element={<RemotePage />} />
          <Route path="/overlay" element={<OverlayPage />} />
          {NAV.map((item) => (
            <Route key={item.to} path={item.to}>
              <Route index element={<SessionListPage mode={item.mode} label={item.label} />} />
              <Route path="new" element={<NewSessionPage mode={item.mode} label={item.label} />} />
              <Route path=":id" element={<SessionDetailPage />} />
            </Route>
          ))}
          <Route path="*" element={<p className="error">Page not found.</p>} />
        </Routes>
      </main>
    </div>
  );
}
