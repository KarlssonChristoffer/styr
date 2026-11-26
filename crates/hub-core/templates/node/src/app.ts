import { Hono } from "hono";

export const app = new Hono();

app.get("/", (c) => c.text("{{name}}"));

app.get("/health", (c) => c.json({ status: "ok" }));
