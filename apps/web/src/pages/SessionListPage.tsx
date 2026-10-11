import { useQuery } from "@tanstack/react-query";
import { Link } from "react-router-dom";
import { listSessions, type SessionMode } from "../api";

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
    <section>
      <div className="row">
        <h2>{label}</h2>
        <Link to={`/${SLUG[mode]}/new`}>New session</Link>
      </div>
      {sessions.isLoading && <p className="muted">Loading sessions…</p>}
      {sessions.isError && <p className="error">Could not load sessions.</p>}
      {sessions.data && sessions.data.sessions.length === 0 && (
        <p className="muted">No sessions in this mode yet.</p>
      )}
      {sessions.data && sessions.data.sessions.length > 0 && (
        <ul className="list">
          {sessions.data.sessions.map((session) => (
            <li key={session.id}>
              <Link to={`/${SLUG[session.mode]}/${session.id}`}>
                <strong>{session.title}</strong>
                <div className="meta">{session.status}</div>
              </Link>
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}
