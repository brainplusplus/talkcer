import { useQuery } from "@tanstack/react-query";
import { NavLink, Navigate, Route, Routes } from "react-router-dom";
import { getHealth, type SessionMode } from "./api";
import { AiConfigPage } from "./pages/AiConfigPage";
import { NewSessionPage } from "./pages/NewSessionPage";
import { OverlayPage } from "./pages/OverlayPage";
import { RemotePage } from "./pages/RemotePage";
import { SessionDetailPage } from "./pages/SessionDetailPage";
import { SessionListPage } from "./pages/SessionListPage";

const NAV: Array<{ to: string; label: string; mode: SessionMode }> = [
  { to: "/live", label: "Live Interview", mode: "live_interview" },
  { to: "/mock", label: "Mock Interview", mode: "mock_interview" },
  { to: "/learning", label: "Learning", mode: "learning" },
];

export function App() {
  const health = useQuery({ queryKey: ["health"], queryFn: getHealth });

  return (
    <div className="app">
      <aside className="sidebar">
        <h1>Talkcer</h1>
        <nav>
          {NAV.map((item) => (
            <NavLink key={item.to} to={item.to}>
              {item.label}
            </NavLink>
          ))}
          <NavLink to="/settings/ai">AI</NavLink>
          <NavLink to="/remote">Remote</NavLink>
          <NavLink to="/overlay">Overlay</NavLink>
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
          <Route path="/settings/ai" element={<AiConfigPage />} />
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
