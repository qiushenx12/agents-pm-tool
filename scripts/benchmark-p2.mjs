import { writeFile } from "node:fs/promises";
import { performance } from "node:perf_hooks";
const base = process.argv[2];
if (!base || !/^http:\/\/(127\.0\.0\.1|localhost):\d+$/.test(base))
  throw new Error(
    "Pass the explicit loopback URL of an isolated preview service.",
  );
async function request(path, body, method = "POST") {
  const response = await fetch(base + "/api/web" + path, {
    method,
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(body),
  });
  if (!response.ok) throw new Error(await response.text());
  return response.status === 204 ? null : response.json();
}
const existing = await (await fetch(base + "/api/web/projects")).json();
const prefix = "P2 性能验收 " + Date.now();
const fixture = [];
for (const count of [100, 1000, 2000]) {
  const project = prefix + " · " + count;
  if (existing.some((p) => p.name === project))
    throw new Error("Fixture project already exists");
  await request("/projects", { name: project, color: "#9162db" });
  const ids = [];
  let next = 0;
  await Promise.all(
    Array.from({ length: 6 }, async () => {
      while (next < count) {
        const index = next++;
        const task = await request("/tasks", {
          project,
          type: index % 2 ? "优化" : "BUG",
          description: "性能验收记录 " + index + "：验证分页、筛选和分组的响应",
        });
        ids.push(task.id);
      }
    }),
  );
  fixture.push({ count, project, ids });
}
const results = [];
for (const item of fixture) {
  const query = "?project=" + encodeURIComponent(item.project);
  const measure = async (suffix) => {
    const times = [];
    let bytes = 0;
    for (let i = 0; i < 6; i++) {
      const start = performance.now();
      const response = await fetch(base + "/api/web/tasks" + suffix + query);
      if (!response.ok) throw new Error(await response.text());
      const text = await response.text();
      JSON.parse(text);
      if (i > 0) times.push(performance.now() - start);
      bytes = Buffer.byteLength(text);
    }
    times.sort((a, b) => a - b);
    return { median_ms: Number(times[2].toFixed(2)), bytes };
  };
  results.push({
    records: item.count,
    legacy: await measure(""),
    paged: await measure("/page"),
  });
}
await writeFile(
  "data/p2-performance-fixtures.json",
  JSON.stringify({ base, fixture, results }, null, 2),
);
console.log(
  JSON.stringify(
    {
      projects: fixture.map(({ count, project }) => ({ count, project })),
      results,
    },
    null,
    2,
  ),
);
