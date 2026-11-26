// What Hub knows about starting a project: the stacks it can start from, and, for "I want to build
// a …", which ones fit with what each is good at and what to watch out for. Hand-written on
// purpose (nothing here asks a model), so the advice is the same every time; change the text here.
//
// A stack with `files: true` is a template the core writes itself (code that builds, a passing
// test, CLAUDE.md, plan); the others are started by the setup thread with the official generator
// (`npm create …`, `composer create-project …`) and the stack named in its first message.

export type Stack = {
  id: string;
  name: string;
  /** Key in `LOGOS`; none draws a folder. */
  logo: string | null;
  /** Language and framework, one short line. */
  sub: string;
  /** The core writes it from its own files (feature `project.templates`); otherwise the setup thread starts it. */
  files: boolean;
  /** What you get, one sentence. */
  blurb: string;
  run?: string;
  test?: string;
  /** Tools the machine needs. */
  needs: string[];
};

export const STACKS: Record<string, Stack> = {
  blank: { id: "blank", name: "Blank", logo: null, sub: "Folder, git and plan", files: true, blurb: "An empty repository with the plan and the living docs. The setup thread picks the stack.", needs: [] },
  fastapi: { id: "fastapi", name: "API server", logo: "fastapi", sub: "Python · FastAPI", files: true, blurb: "A small JSON API with a /health route and a test that passes.", run: "uv run fastapi dev", test: "uv run pytest", needs: ["python", "uv"] },
  php: { id: "php", name: "PHP site", logo: "php", sub: "PHP 8 · Composer", files: true, blurb: "Plain PHP with a front controller, the built-in dev server and a PHPUnit test.", run: "php -S localhost:8000 -t public", test: "composer test", needs: ["php", "composer"] },
  node: { id: "node", name: "Node service", logo: "hono", sub: "TypeScript · Hono", files: true, blurb: "A small HTTP service in TypeScript with a route and a test that passes.", run: "npm run dev", test: "npm test", needs: ["node", "npm"] },
  python: { id: "python", name: "Python", logo: "python", sub: "Package · pytest · ruff", files: true, blurb: "A package with an entry point, tests and a linter.", run: "uv run python -m app", test: "uv run pytest", needs: ["python", "uv"] },
  go: { id: "go", name: "Go service", logo: "go", sub: "Go · net/http", files: true, blurb: "A small HTTP service on the standard library with a test.", run: "go run .", test: "go test ./...", needs: ["go"] },
  rust: { id: "rust", name: "Rust tool", logo: "rust", sub: "Command line · clap", files: true, blurb: "A command line tool with arguments and a test.", run: "cargo run", test: "cargo test", needs: ["cargo"] },
  static: { id: "static", name: "Static page", logo: "html5", sub: "HTML · CSS", files: true, blurb: "One page with a stylesheet, nothing to install.", run: "python -m http.server", needs: [] },
  sveltekit: { id: "sveltekit", name: "Web app", logo: "svelte", sub: "SvelteKit · TypeScript", files: false, blurb: "SvelteKit from its own generator, with checks wired in.", run: "npm run dev", test: "npm run check", needs: ["node", "npm"] },
  react: { id: "react", name: "React app", logo: "react", sub: "React · Vite · TypeScript", files: false, blurb: "React on Vite from its own generator.", run: "npm run dev", test: "npm test", needs: ["node", "npm"] },
  laravel: { id: "laravel", name: "Laravel app", logo: "laravel", sub: "Laravel · PHP", files: false, blurb: "Laravel from Composer, with login, queues and a database ready.", run: "php artisan serve", test: "php artisan test", needs: ["php", "composer"] },
  nextjs: { id: "nextjs", name: "Next.js", logo: "nextdotjs", sub: "React · TypeScript", files: false, blurb: "Next.js from create-next-app.", run: "npm run dev", test: "npm test", needs: ["node", "npm"] },
  astro: { id: "astro", name: "Astro", logo: "astro", sub: "Content sites", files: false, blurb: "Astro from its own generator.", run: "npm run dev", needs: ["node", "npm"] },
  django: { id: "django", name: "Django", logo: "django", sub: "Python", files: false, blurb: "Django with its admin.", run: "uv run manage.py runserver", test: "uv run manage.py test", needs: ["python", "uv"] },
  tauri: { id: "tauri", name: "Tauri", logo: "tauri", sub: "Rust + a web UI", files: false, blurb: "A desktop app with a web UI, from create-tauri-app.", run: "npm run tauri dev", needs: ["node", "cargo"] },
  dotnet: { id: "dotnet", name: ".NET", logo: "dotnet", sub: "C# · WinUI", files: false, blurb: "A Windows app from the .NET templates.", run: "dotnet run", test: "dotnet test", needs: ["dotnet"] },
  electron: { id: "electron", name: "Electron", logo: "electron", sub: "JavaScript · web UI", files: false, blurb: "A desktop app from Electron Forge.", run: "npm start", needs: ["node", "npm"] },
  flutter: { id: "flutter", name: "Flutter", logo: "flutter", sub: "Dart", files: false, blurb: "One code base for phone, desktop and web.", run: "flutter run", test: "flutter test", needs: ["flutter"] },
  expo: { id: "expo", name: "Expo", logo: "expo", sub: "React Native · TypeScript", files: false, blurb: "An iPhone and Android app from create-expo-app.", run: "npx expo start", needs: ["node", "npm"] },
  swift: { id: "swift", name: "Swift", logo: "swift", sub: "iPhone and Mac", files: false, blurb: "A Swift package or app; builds on a Mac.", run: "swift run", test: "swift test", needs: ["swift"] },
  godot: { id: "godot", name: "Godot", logo: "godotengine", sub: "GDScript", files: false, blurb: "A Godot project that opens in the editor.", needs: ["godot"] },
  unity: { id: "unity", name: "Unity", logo: "unity", sub: "C#", files: false, blurb: "A Unity project, made in the Unity Hub.", needs: ["unity"] },
  shell: { id: "shell", name: "Shell scripts", logo: "gnubash", sub: "Bash", files: false, blurb: "A folder of scripts with a runner and shellcheck.", test: "shellcheck *.sh", needs: ["bash"] },
  jupyter: { id: "jupyter", name: "Notebook", logo: "jupyter", sub: "Python · Jupyter", files: false, blurb: "A notebook project with its environment.", run: "uv run jupyter lab", needs: ["python", "uv"] },
};

/** The templates in the picker's grid, in order. The rest are reached through the advice. */
export const GRID = ["blank", "fastapi", "php", "laravel", "sveltekit", "react", "node", "python", "go", "rust", "static"];

export type Advice = { stack: string; good: string; watch: string; recommended?: boolean };

/** "I want to build a …": what Hub suggests for each, the recommended one first. */
export const KINDS: { kind: string; advice: Advice[] }[] = [
  {
    kind: "Web app",
    advice: [
      { stack: "sveltekit", recommended: true, good: "small and fast, very little boilerplate, what Hub itself is built with", watch: "smaller ecosystem than React; fewer ready-made components" },
      { stack: "nextjs", good: "biggest ecosystem, server rendering built in, easy to find help", watch: "heavy, changes often, easy to build more than you need" },
      { stack: "laravel", good: "everything included (login, queues, database), quick to ship, cheap hosting", watch: "the back end is PHP only; the front end is a second choice" },
      { stack: "django", good: "batteries included with an admin for free, and Python all the way", watch: "less suited to rich, app-like screens" },
    ],
  },
  {
    kind: "Website",
    advice: [
      { stack: "astro", recommended: true, good: "ships almost no JavaScript, great for blogs and docs", watch: "not meant for app-like screens" },
      { stack: "static", good: "nothing to install and nothing to break", watch: "repeats itself once there are more than a few pages" },
      { stack: "nextjs", good: "a site that can grow into an app", watch: "more machinery than a plain site needs" },
    ],
  },
  {
    kind: "API or service",
    advice: [
      { stack: "fastapi", recommended: true, good: "fast to write, typed, docs generated for you", watch: "Python is slower than Go or Rust under heavy load" },
      { stack: "node", good: "TypeScript front to back, tiny and quick to start", watch: "a young framework, fewer examples to copy" },
      { stack: "go", good: "one small binary, very fast, easy to deploy", watch: "more typing for the same thing, fewer libraries" },
      { stack: "php", good: "runs on any cheap host, simple to start", watch: "easy to end up with a tangle without a framework" },
    ],
  },
  {
    kind: "Mobile app",
    advice: [
      { stack: "expo", recommended: true, good: "TypeScript, iPhone and Android from one code base, try it on your phone at once", watch: "native extras need a custom build" },
      { stack: "flutter", good: "one code base, smooth animations, phone, desktop and web", watch: "looks its own way, not native; Dart is a smaller world" },
      { stack: "swift", good: "the best iPhone feel and every new Apple feature first", watch: "Apple only, and it needs a Mac to build" },
    ],
  },
  {
    kind: "Windows app",
    advice: [
      { stack: "tauri", recommended: true, good: "installer about 10 MB, UI in Svelte or React, what Hub uses", watch: "needs Rust for the native part" },
      { stack: "dotnet", good: "the most native Windows feel and the best tooling", watch: "Windows first; Mac and Linux are second class" },
      { stack: "electron", good: "all web, huge ecosystem, no Rust needed", watch: "150 MB+ installer and a lot of memory" },
      { stack: "flutter", good: "one code base for Windows, Mac, phone and web", watch: "looks its own way, not native; Dart is a smaller world" },
    ],
  },
  {
    kind: "Mac or Linux app",
    advice: [
      { stack: "tauri", recommended: true, good: "small, fast, one code base for Mac, Linux and Windows", watch: "needs Rust for the native part" },
      { stack: "electron", good: "all web, huge ecosystem, no Rust needed", watch: "150 MB+ installer and a lot of memory" },
      { stack: "swift", good: "a real Mac app with every system feature", watch: "Mac only" },
    ],
  },
  {
    kind: "Command line tool",
    advice: [
      { stack: "rust", recommended: true, good: "one fast binary with no runtime to install, good error messages", watch: "slower to write, longer builds" },
      { stack: "go", good: "one binary, builds in seconds, easy to cross-compile", watch: "fewer ways to model things than Rust" },
      { stack: "python", good: "quickest to write, a library for everything", watch: "needs Python on the machine that runs it" },
    ],
  },
  {
    kind: "Game",
    advice: [
      { stack: "godot", recommended: true, good: "free and light, 2D and 3D, one file format that agents can read", watch: "a smaller asset world than Unity" },
      { stack: "unity", good: "the biggest asset store and the most guides", watch: "heavy, a licence to read, and projects agents find hard to edit" },
    ],
  },
  {
    kind: "Automation script",
    advice: [
      { stack: "python", recommended: true, good: "readable, a library for everything, runs anywhere", watch: "needs Python on the machine that runs it" },
      { stack: "node", good: "TypeScript if the rest of your work is, good for web calls", watch: "needs Node and packages installed" },
      { stack: "shell", good: "nothing to install, glue between other tools", watch: "gets fragile fast once it grows" },
    ],
  },
  {
    kind: "AI or data",
    advice: [
      { stack: "python", recommended: true, good: "every AI and data library lands here first", watch: "environments and packages need care" },
      { stack: "jupyter", good: "try things step by step and see the result at once", watch: "notebooks are hard to review and test" },
    ],
  },
];

/** What to tell the setup thread when the core cannot write the template itself. */
export function stackHint(id: string): string {
  const s = STACKS[id];
  if (!s || id === "blank") return "";
  const how = s.files
    ? "Write it by hand, small and complete."
    : "Start it with the stack's own official generator, not by hand.";
  const cmds = [s.run && `run \`${s.run}\``, s.test && `test \`${s.test}\``].filter(Boolean).join(", ");
  return `Use this stack: ${s.name} (${s.sub}). ${how} ${s.blurb}${cmds ? ` Put how to build, run and test it in CLAUDE.md (${cmds}).` : ""}`;
}
