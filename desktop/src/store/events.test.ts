import { useEventStore } from "./events";

test("ingesting an event updates status and recent list", () => {
  useEventStore.getState().setStatus("open");
  useEventStore.getState().push({ type: "memory_created", id: "mem_9", title: "X", tier: "semantic" });
  const s = useEventStore.getState();
  expect(s.status).toBe("open");
  expect(s.recent[0]).toMatchObject({ type: "memory_created", id: "mem_9" });
});
