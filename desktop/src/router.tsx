import { createRootRoute, createRoute, createRouter, Outlet } from "@tanstack/react-router";
import { Shell } from "./layout/Shell";
import * as V from "./views";

const rootRoute = createRootRoute({ component: () => (<Shell><Outlet /></Shell>) });
const r = (path: string, component: () => JSX.Element) => createRoute({ getParentRoute: () => rootRoute, path, component });

const routes = [
  r("/", V.Browser), r("/search", V.Search), r("/graph", V.Graph), r("/timeline", V.Timeline),
  r("/pipelines", V.Pipelines), r("/reflections", V.Reflections), r("/audit", V.Audit),
  // Editor/EntityProfile accept an optional `id` prop for test isolation;
  // the router calls them with no args so wrap with a no-arg closure.
  r("/editor/$id", () => <V.Editor />), r("/entity/$id", () => <V.EntityProfile />),
];
const routeTree = rootRoute.addChildren(routes);
export const router = createRouter({ routeTree });
declare module "@tanstack/react-router" { interface Register { router: typeof router; } }
