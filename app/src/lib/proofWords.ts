// What a reply's proof stamp says (ProofStamp.svelte): the tone, the short text and the tip.
// A proof that came from a look at a page (`page_check`, `via: "screenshot"`) reads as a page;
// any other or unknown `via` keeps the words for a test run, so an older or newer hubd never
// breaks the stamp.
import type { ProofState } from "./transcriptItems";

export type ProofWords = { tone: "done" | "busy" | "bad" | "none"; text: string; title: string };

export function proofWords(proof: ProofState): ProofWords {
  // Only a look with page_check can fail as a screenshot (`proof.rs`), so a failed or asked-to-fix
  // screenshot is a page that had errors.
  const page = proof.via === "screenshot";
  const ran = proof.command ?? "The last test run";
  switch (proof.state) {
    case "shown":
      return { tone: "done", text: page ? "screenshot" : "tested", title: proof.command ? `Proof: ${proof.command}` : "Showed that it works" };
    case "asked":
      if (proof.hidden) return { tone: "busy", text: "checking test output…", title: `${ran} exited cleanly but its output reads as a failure; Hub asked the agent to look` };
      if (proof.why === "failed") {
        return page
          ? { tone: "busy", text: "fixing page errors…", title: `The page had errors: ${proof.command ?? ""}` }
          : { tone: "busy", text: "fixing failed tests…", title: `The last test run failed: ${proof.command ?? ""}` };
      }
      return { tone: "busy", text: "asked for proof…", title: "It changed code without running tests; Hub asked it to show that it works" };
    case "skipped":
      return {
        tone: "none",
        text: "no proof needed",
        title:
          proof.why === "waived" ? "You said not to test this time"
          : proof.why === "reverted" ? "It put back what an earlier turn changed, so there is nothing new to test"
          : proof.why === "scratch" ? "It worked in a scratch folder, where nothing ships, so Hub does not ask for proof"
          : "It only took code out, so there is nothing new to test",
      };
    case "failed":
      if (proof.hidden) return { tone: "bad", text: "tests failed", title: `${ran} exited cleanly but printed failures` };
      if (page) return { tone: "bad", text: "page has errors", title: proof.command ? `Errors: ${proof.command}` : "The last look at the page found errors" };
      return { tone: "bad", text: "tests failed", title: proof.command ? `Failed: ${proof.command}` : "The last test run failed" };
    default:
      return { tone: "none", text: "no proof", title: "Changed code without tests or a screenshot, even after Hub asked" };
  }
}
