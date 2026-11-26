import { expect, test } from "vitest";
import { app } from "./app";

test("health is ok", async () => {
  const res = await app.request("/health");
  expect(res.status).toBe(200);
  expect(await res.json()).toEqual({ status: "ok" });
});

test("the front page names the service", async () => {
  const res = await app.request("/");
  expect(res.status).toBe(200);
  expect(await res.text()).toContain("{{name}}");
});
