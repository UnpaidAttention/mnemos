import { Component, type ReactNode } from "react";

interface State {
  error: Error | null;
}

export class ErrorBoundary extends Component<{ children: ReactNode }, State> {
  state: State = { error: null };

  static getDerivedStateFromError(error: Error): State {
    return { error };
  }

  render() {
    if (this.state.error) {
      return (
        <div
          role="alert"
          className="m-6 rounded-xl border shadow-raised p-5 space-y-2"
          style={{
            background: "var(--surface)",
            borderColor: "color-mix(in srgb, var(--tier-procedural) 30%, var(--border))",
          }}
        >
          <h2
            className="display text-base"
            style={{ color: "var(--tier-procedural)" }}
          >
            Something went wrong
          </h2>
          <pre
            className="mono text-xs whitespace-pre-wrap"
            style={{ color: "var(--text-muted)" }}
          >
            {this.state.error.message}
          </pre>
          <button
            className="label text-xs focus-visible:outline focus-visible:outline-2 focus-visible:outline-accent"
            style={{ color: "var(--accent)" }}
            onClick={() => this.setState({ error: null })}
          >
            Try again
          </button>
        </div>
      );
    }
    return this.props.children;
  }
}
