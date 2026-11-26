// Loads the fonts before the app mounts, so the first paint already has them.
import "@fontsource-variable/geist";
import "@fontsource-variable/geist-mono";
import "@fontsource-variable/newsreader";
import "./app.css";
import { mount } from "svelte";
import App from "./App.svelte";
import ViewerWindow from "./lib/components/ViewerWindow.svelte";
import TestWindow from "./lib/components/TestWindow.svelte";
import InstallWindow from "./lib/components/InstallWindow.svelte";
import { poppedViewing } from "./lib/native";
import { installInputMode } from "./lib/inputMode";
import { installProblemReports } from "./lib/problems";
import { installTooltips } from "./lib/tooltip";

installInputMode();
installTooltips();
installProblemReports();

// The card the shell shows on the desktop while it installs a new version shows just that; a
// viewer popped out of the main window runs on its own, showing just that.
const viewing = poppedViewing();
if ((window as { __HUB_INSTALL_CARD__?: boolean }).__HUB_INSTALL_CARD__) mount(InstallWindow, { target: document.getElementById("app")! });
else if (viewing?.kind === "tests") mount(TestWindow, { target: document.getElementById("app")!, props: JSON.parse(viewing.id) });
else if (viewing) mount(ViewerWindow, { target: document.getElementById("app")!, props: { viewing } });
else mount(App, { target: document.getElementById("app")! });

// Loops pause while nothing is shown (animation rule: infinite animations only run when seen).
const markHidden = () => document.documentElement.toggleAttribute("data-hidden", document.hidden);
document.addEventListener("visibilitychange", markHidden);
markHidden();
