export function PlaceholderPage({ title, note }: { title: string; note: string }) {
  return (
    <section className="page">
      <header className="page-head">
        <div>
          <h2>{title}</h2>
          <p className="lede">{note}</p>
        </div>
      </header>
    </section>
  );
}
