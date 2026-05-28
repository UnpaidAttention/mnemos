import { useEffect } from "react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { RouterProvider } from "@tanstack/react-router";
import { ThemeProvider } from "./design/ThemeProvider";
import { router } from "./router";
import { connectEvents } from "./api/ws";

const queryClient = new QueryClient({ defaultOptions: { queries: { staleTime: 10_000, retry: 1 } } });

export default function App() {
  useEffect(() => connectEvents(queryClient), []);
  return (
    <ThemeProvider>
      <QueryClientProvider client={queryClient}>
        <RouterProvider router={router} />
      </QueryClientProvider>
    </ThemeProvider>
  );
}
