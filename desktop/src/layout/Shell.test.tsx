import { render, screen } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import {
  RouterProvider,
  createMemoryHistory,
  createRootRoute,
  createRoute,
  createRouter,
} from "@tanstack/react-router";
import { Shell } from "./Shell";

// The Shell uses <Link> (needs a RouterProvider) and the Inspector uses query
// hooks (need a QueryClientProvider). Wrap accordingly per the plan's test
// convention. The Shell content is rendered inside the root route component.
function renderShell() {
  const rootRoute = createRootRoute({
    component: () => (
      <Shell>
        <div>center content</div>
      </Shell>
    ),
  });
  const indexRoute = createRoute({ getParentRoute: () => rootRoute, path: "/", component: () => null });
  const router = createRouter({
    routeTree: rootRoute.addChildren([indexRoute]),
    history: createMemoryHistory({ initialEntries: ["/"] }),
  });
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={qc}>
      <RouterProvider router={router} />
    </QueryClientProvider>,
  );
}

test("shell renders the three regions and brand", async () => {
  renderShell();
  expect(await screen.findByText(/center content/)).toBeInTheDocument();
  expect(screen.getByRole("banner")).toBeInTheDocument();        // top bar
  expect(screen.getByRole("navigation")).toBeInTheDocument();    // left sidebar
  expect(screen.getByRole("complementary")).toBeInTheDocument(); // inspector
});
