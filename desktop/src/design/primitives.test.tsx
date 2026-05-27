import { render, screen } from "@testing-library/react";
import { TierChip, Button } from "./primitives";

test("TierChip shows the tier label and carries the tier data attribute", () => {
  render(<TierChip tier="semantic" />);
  const chip = screen.getByText(/semantic/i);
  expect(chip).toBeInTheDocument();
  expect(chip.closest("[data-tier]")).toHaveAttribute("data-tier", "semantic");
});

test("Button renders children and fires onClick", async () => {
  const { default: userEvent } = await import("@testing-library/user-event");
  const onClick = vi.fn();
  render(<Button onClick={onClick}>Save</Button>);
  await userEvent.click(screen.getByRole("button", { name: "Save" }));
  expect(onClick).toHaveBeenCalledOnce();
});
