import { useMutation, useQueryClient } from "@tanstack/react-query";
import { FormEvent, useState } from "react";
import { Link, useNavigate } from "react-router-dom";
import { createSession, type SessionMode } from "../api";

const SLUG: Record<SessionMode, string> = {
  live_interview: "live",
  mock_interview: "mock",
  learning: "learning",
};

export function NewSessionPage({ mode, label }: { mode: SessionMode; label: string }) {
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const [title, setTitle] = useState("");
  const [role, setRole] = useState("");
  const [company, setCompany] = useState("");
  const [language, setLanguage] = useState("en");
  const [error, setError] = useState<string | null>(null);

  const mutation = useMutation({
    mutationFn: createSession,
    onSuccess: async (session) => {
      await queryClient.invalidateQueries({ queryKey: ["sessions"] });
      await navigate(`/${SLUG[session.mode]}/${session.id}`);
    },
    onError: (cause: Error) => setError(cause.message),
  });

  function onSubmit(event: FormEvent) {
    event.preventDefault();
    setError(null);
    mutation.mutate({
      title,
      mode,
      context: { role, company, language },
      configuration: { stt: "default", thinker: "default", tts: "disabled" },
    });
  }

  return (
    <section className="page">
      <header className="page-head">
        <div>
          <h2>New {label}</h2>
          <p className="lede">A title is enough. Role and company stay with this session.</p>
        </div>
        <Link to={`/${SLUG[mode]}`}>Back</Link>
      </header>
      <form onSubmit={onSubmit} className="sheet pad">
        <label>
          Title
          <input value={title} onChange={(event) => setTitle(event.target.value)} required />
        </label>
        <label>
          Role
          <input value={role} onChange={(event) => setRole(event.target.value)} />
        </label>
        <label>
          Company
          <input value={company} onChange={(event) => setCompany(event.target.value)} />
        </label>
        <label>
          Language
          <input value={language} onChange={(event) => setLanguage(event.target.value)} />
        </label>
        {error && <p className="error">{error}</p>}
        <button type="submit" className="primary" disabled={mutation.isPending}>
          {mutation.isPending ? "Creating…" : "Create session"}
        </button>
      </form>
    </section>
  );
}
