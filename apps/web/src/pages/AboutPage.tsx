import { useQuery } from "@tanstack/react-query";
import { getHealth } from "../api";

export function AboutPage() {
  const health = useQuery({ queryKey: ["health"], queryFn: getHealth });

  return (
    <section className="page">
      <header className="page-head">
        <div>
          <h2>About</h2>
          <p className="lede">Talkcer is a web app with a desktop shell around it.</p>
        </div>
      </header>
      <div className="sheet pad">
        <p className="lede">
          The desktop window adds what a browser tab cannot: the global shortcuts, the overlay,
          and undetectable mode. Everything else, including this page, is the same web app a
          second computer opens.
        </p>
        {health.data && (
          <p className="meta">
            {health.data.app} {health.data.version}
          </p>
        )}
      </div>
    </section>
  );
}
