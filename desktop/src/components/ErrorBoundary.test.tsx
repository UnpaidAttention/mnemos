import { render, screen } from "@testing-library/react";
import { ErrorBoundary } from "./ErrorBoundary";

function Boom(): JSX.Element {
  throw new Error("boom");
}

test("renders fallback when a child throws", () => {
  const spy = vi.spyOn(console, "error").mockImplementation(() => {});
  render(
    <ErrorBoundary>
      <Boom />
    </ErrorBoundary>,
  );
  expect(screen.getByRole("alert")).toHaveTextContent(/something went wrong/i);
  spy.mockRestore();
});

test("renders children when no error", () => {
  render(
    <ErrorBoundary>
      <span>all good</span>
    </ErrorBoundary>,
  );
  expect(screen.getByText("all good")).toBeInTheDocument();
});
