import { useEffect, useState } from "react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { RouterProvider } from "@tanstack/react-router";
import { ThemeProvider } from "./design/ThemeProvider";
import { router } from "./router";
import { connectEvents } from "./api/ws";
import { client } from "./api/client";
import { FirstRun } from "./views/FirstRun";
import { UpdateBanner } from "./components/UpdateBanner";

const queryClient = new QueryClient({ defaultOptions: { queries: { staleTime: 10_000, retry: 1 } } });

export default function App() {
  useEffect(() => connectEvents(queryClient), []);
  // null = unchecked, true = show wizard, false = wizard dismissed
  const [firstRunShown, setFirstRunShown] = useState<boolean | null>(null);
  useEffect(() => {
    void client
      .getFirstRun()
      .then((r) => setFirstRunShown(r.completed_at == null))
      .catch(() => setFirstRunShown(false));
  }, []);
  return (
    <ThemeProvider>
      <QueryClientProvider client={queryClient}>
        <UpdateBanner />
        <RouterProvider router={router} />
        {firstRunShown === true && <FirstRun onClose={() => setFirstRunShown(false)} />}
      </QueryClientProvider>
    </ThemeProvider>
  );
}
