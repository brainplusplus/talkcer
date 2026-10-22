import { useQuery } from "@tanstack/react-query";
import { Link } from "react-router-dom";
import { listSessions, listSessionsPath, type SessionMode } from "../api";

const SLUG: Record<SessionMode, string> = {
  live_interview: "live",
  mock_interview: "mock",
  learning: "learning",
};

export function SessionListPage({ mode, label }: { mode: SessionMode; label: string }) {
  const sessions = useQuery({
    queryKey: ["sessions", mode],
    queryFn: () => listSessions(mode),
  });

  return (
    <section className="page">
      <header className="page-head">
        <div>
          <h2>{label}</h2>
          <p className="lede">Sessions in this mode. Open one to continue it.</p>
        </div>
        <Link to={`/${SLUG[mode]}/new`}>New session</Link>
      </header>
      {sessions.isLoading && <p className="muted">Loading sessions…</p>}
      {sessions.isError && <p className="error">Could not load sessions.</p>}
      {sessions.data && sessions.data.sessions.length === 0 && (
        <p className="muted">No sessions in this mode yet.</p>
      )}
      {sessions.data && sessions.data.sessions.length > 0 && (
        <div className="sheet">
          <ul className="list">
            {sessions.data.sessions.map((session) => (
              <li key={session.id} data-testid="session-link">
                <Link to={`/${SLUG[session.mode]}/${session.id}`}>
                  <strong>{session.title}</strong>
                  <span className="meta">{session.status}</span>
                </Link>
              </li>
            ))}
          </ul>
        </div>
      )}
      <p className="note">API endpoint: <code>{listSessionsPath(mode)}</code></p>
    </section>
  );
}
