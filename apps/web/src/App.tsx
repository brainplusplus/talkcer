import { useQuery } from "@tanstack/react-query";
import { NavLink, Navigate, Route, Routes, useLocation } from "react-router-dom";
import { currentSession, getHealth, type SessionMode } from "./api";
import { AboutPage } from "./pages/AboutPage";
import { AiConfigPage } from "./pages/AiConfigPage";
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
          <NavLink to="/settings/profile">Profile</NavLink>
          <NavLink to="/settings/ai">General</NavLink>
          <NavLink to="/settings/privacy">Privacy</NavLink>
          <NavLink to="/overlay">Overlay</NavLink>
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
          <Route path="/settings/profile" element={<ProfilePage />} />
          <Route path="/about" element={<AboutPage />} />
          <Route path="/settings/ai" element={<AiConfigPage />} />
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
