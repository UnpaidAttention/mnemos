import { render, screen } from "@testing-library/react";
import App from "./App";

test("renders the app shell with the mnemos brand", async () => {
  render(<App />);
  expect(await screen.findByText(/mnemos/i)).toBeInTheDocument();
});
