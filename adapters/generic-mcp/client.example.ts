// Minimal MCP client connecting to mnemos over the daemon's streamable HTTP
// endpoint. Run: `npx tsx client.example.ts` after `npm i @modelcontextprotocol/sdk`.
import { Client } from "@modelcontextprotocol/sdk/client/index.js";
import { StreamableHTTPClientTransport } from "@modelcontextprotocol/sdk/client/streamableHttp.js";
import * as fs from "node:fs";
import * as os from "node:os";
import * as path from "node:path";

async function main(): Promise<void> {
  const tokenPath = path.join(os.homedir(), ".config", "mnemos", "token");
  const token = fs.readFileSync(tokenPath, "utf8").trim();
  const transport = new StreamableHTTPClientTransport(
    new URL("http://localhost:7423/mcp"),
    { requestInit: { headers: { authorization: `Bearer ${token}` } } },
  );
  const client = new Client(
    { name: "mnemos-generic-example", version: "0.1.0" },
    { capabilities: {} },
  );
  await client.connect(transport);

  const tools = await client.listTools();
  console.log("tools:", tools.tools.map((t) => t.name).join(", "));

  const rem = await client.callTool({
    name: "remember",
    arguments: { body: "Hello from generic-mcp", tier: "semantic" },
  });
  console.log("remember →", rem);

  const hits = await client.callTool({
    name: "recall",
    arguments: { query: "hello", k: 5 },
  });
  console.log("recall →", hits);

  await client.close();
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});
