import { vitePreprocess } from "@sveltejs/vite-plugin-svelte";
import { parse } from "svelte/compiler";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

/** Puts the component's file on the root elements it draws (`data-c="lib/components/Rail.svelte"`),
 *  so Inspect mode can tell which component an element on screen belongs to, in the installed
 *  build too. Only elements that are direct output of the component (not inside child
 *  components); blocks (`{#if}`, `{#each}`, snippets) are looked through. */
function componentNames() {
  const ROOT = /[\\/]app[\\/]src[\\/](.*)$/;
  const BLOCKS = ["consequent", "alternate", "body", "fallback", "pending", "then", "catch", "children"];
  return {
    name: "hub-component-names",
    markup({ content, filename }) {
      const m = filename && ROOT.exec(filename);
      if (!m || !filename.endsWith(".svelte")) return;
      let ast;
      try {
        ast = parse(content, { modern: true, filename });
      } catch {
        return;
      }
      const at = [];
      const look = (node) => {
        if (!node || typeof node !== "object") return;
        if (Array.isArray(node)) return node.forEach(look);
        if (node.type === "RegularElement") {
          // `<svelte:element>`, components and slots are other types; a plain element is marked and not looked into.
          if (!/^[a-z]/.test(node.name)) return;
          at.push(node.start + 1 + node.name.length);
          return;
        }
        if (node.type === "Fragment") return look(node.nodes);
        if (node.type === "SnippetBlock") return look(node.body);
        if (/Block$/.test(node.type)) for (const k of BLOCKS) look(node[k]);
      };
      look(ast.fragment);
      let out = content;
      const file = m[1].replace(/\\/g, "/");
      for (const i of at.sort((a, b) => b - a)) out = `${out.slice(0, i)} data-c="${file}"${out.slice(i)}`;
      return { code: out };
    },
  };
}

/** Pastes shared CSS into the style blocks of a family of components. A style block that holds the
 *  comment `@kit name` gets `components/kits/name.css` there (the Settings pages share `settings`, the
 *  composer's menus `menus`). Scoped to each component, so it cannot leak into the components inside
 *  it, which a plain stylesheet would; a component that leaves a rule unused just has it dropped (the
 *  unused-selector warning is off for the components that use a kit, below). */
function pasteKit() {
  const KIT = /\/\*\s*@kit (\w+)\s*\*\//;
  const dir = fileURLToPath(new URL("./src/lib/components/kits/", import.meta.url));
  return {
    name: "hub-paste-kit",
    style({ content }) {
      const m = KIT.exec(content);
      if (!m) return;
      const file = `${dir}${m[1]}.css`;
      const css = readFileSync(file, "utf8").replace(/^\/\*[\s\S]*?\*\/\n/, "");
      return { code: content.replace(KIT, css), dependencies: [file] };
    },
  };
}

const kitUsers = new Map();
/** Whether the file holds an `@kit` comment (read once per file). */
function usesKit(file) {
  if (!file) return false;
  if (!kitUsers.has(file)) {
    let has = false;
    try {
      has = /\/\*\s*@kit \w+\s*\*\//.test(readFileSync(file, "utf8"));
    } catch {}
    kitUsers.set(file, has);
  }
  return kitUsers.get(file);
}

export default {
  preprocess: [vitePreprocess(), componentNames(), pasteKit()],
  // A component that pastes a kit gets all of it and uses part of it.
  compilerOptions: { warningFilter: (w) => !(w.code === "css_unused_selector" && usesKit(w.filename)) },
};
