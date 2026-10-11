import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { FormEvent, useState } from "react";
import { pairRemote, remoteSessions, remoteStatus, type Session } from "../api";

export function RemotePage() {
  const queryClient = useQueryClient();
  const status = useQuery({ queryKey: ["remote-status"], queryFn: remoteStatus });
  const [token, setToken] = useState("");
  const [shownToken, setShownToken] = useState<string | null>(null);
  const [sessions, setSessions] = useState<Session[] | null>(null);
  const [error, setError] = useState<string | null>(null);

  const pair = useMutation({
    mutationFn: (enabled: boolean) => pairRemote(enabled),
    onSuccess: async (result) => {
      setShownToken(result.token ?? null);
      setError(null);
      await queryClient.invalidateQueries({ queryKey: ["remote-status"] });
    },
    onError: (cause: Error) => setError(cause.message),
  });

  async function loadRemote(event: FormEvent) {
    event.preventDefault();
    setError(null);
    try {
      const result = await remoteSessions(token);
      setSessions(result.sessions);
    } catch (cause) {
      setSessions(null);
      setError(cause instanceof Error ? cause.message : "Remote access denied.");
    }
  }

  return (
    <section className="stack">
      <h2>Remote access</h2>
      <p className="meta">{status.data?.enabled ? "Pairing is on." : "Pairing is off. A second computer is denied."}</p>
      <div className="row">
        <button type="button" onClick={() => pair.mutate(true)}>Pair this computer</button>
        <button type="button" onClick={() => pair.mutate(false)}>Disable remote</button>
      </div>
      {shownToken && <p>Token, shown once: <code>{shownToken}</code></p>}
      <form onSubmit={(event) => void loadRemote(event)}>
        <label>
          Second-computer token
          <input value={token} onChange={(event) => setToken(event.target.value)} />
        </label>
        <button type="submit">Load sessions as second computer</button>
      </form>
      {sessions && (
        <ul className="list">
          {sessions.map((session) => (
            <li key={session.id}>{session.title} · {session.mode}</li>
          ))}
        </ul>
      )}
      {error && <p className="error">{error}</p>}
    </section>
  );
}
