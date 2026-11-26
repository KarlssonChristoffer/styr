// Which folded sections of a machine's page are open, remembered per machine in this window.
const KEY = "hub.machine-folds";

function read(): Record<string, boolean> {
  try {
    return JSON.parse(localStorage.getItem(KEY) ?? "{}");
  } catch {
    return {};
  }
}

class Folds {
  private state = $state<Record<string, boolean>>(read());

  isOpen(machine: string, section: string, open: boolean): boolean {
    return this.state[`${machine}|${section}`] ?? open;
  }

  toggle(machine: string, section: string, open: boolean) {
    this.state[`${machine}|${section}`] = !this.isOpen(machine, section, open);
    try {
      localStorage.setItem(KEY, JSON.stringify(this.state));
    } catch {
      // Private window or blocked storage: it just does not remember.
    }
  }
}

export const folds = new Folds();
