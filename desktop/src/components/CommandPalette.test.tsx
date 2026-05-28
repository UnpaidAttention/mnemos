import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { renderWithQuery } from "../test/renderWithQuery";
import { CommandPalette } from "./CommandPalette";

vi.mock("@tanstack/react-router", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@tanstack/react-router")>();
  return { ...actual, useNavigate: () => vi.fn() };
});

test("filters commands by typed text", async () => {
  renderWithQuery(<CommandPalette open onClose={() => {}} />);
  // router renders asynchronously — wait for the input to appear
  const input = await screen.findByPlaceholderText(/type a command/i);
  await userEvent.type(input, "graph");
  expect(screen.getByText(/open graph/i)).toBeInTheDocument();
  expect(screen.queryByText(/open audit/i)).not.toBeInTheDocument();
});

test("shows all commands when query is empty", async () => {
  renderWithQuery(<CommandPalette open onClose={() => {}} />);
  await screen.findByPlaceholderText(/type a command/i);
  expect(screen.getByText(/open graph/i)).toBeInTheDocument();
  expect(screen.getByText(/open audit/i)).toBeInTheDocument();
  expect(screen.getByText(/toggle inspector/i)).toBeInTheDocument();
});

test("returns null when closed", async () => {
  renderWithQuery(<CommandPalette open={false} onClose={() => {}} />);
  // router renders; palette should not appear
  await screen.findByRole("presentation", { hidden: true }).catch(() => {});
  expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
});

test("calls onClose when Escape is pressed", async () => {
  const onClose = vi.fn();
  renderWithQuery(<CommandPalette open onClose={onClose} />);
  const input = await screen.findByPlaceholderText(/type a command/i);
  input.focus();
  await userEvent.keyboard("{Escape}");
  expect(onClose).toHaveBeenCalledOnce();
});
