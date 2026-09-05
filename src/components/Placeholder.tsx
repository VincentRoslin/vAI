interface Props {
  title: string;
  phase: string;
  children?: React.ReactNode;
}

/**
 * Phase-6 placeholder for a not-yet-built surface. Names the feature and the
 * roadmap phase that implements it, so the shell is navigable and honest.
 */
export function Placeholder({ title, phase, children }: Props): React.JSX.Element {
  return (
    <section>
      <h1 style={{ marginTop: 0 }}>{title}</h1>
      <p style={{ color: 'var(--text-muted)' }}>
        Not built yet — arrives in <code>{phase}</code>.
      </p>
      {children}
    </section>
  );
}
