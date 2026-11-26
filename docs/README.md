# Documentation
<!-- för: Christoffer -->

These are the working documents of the project. Most of them are written for the coding agents
that work in the repository, so they are dense and exact about file names rather than
introductory. Start with the [README](../README.md) for what the app is, then pick from the
table.

| Document | What it covers |
|---|---|
| [INSTALL.md](INSTALL.md) | Installing on Windows, how updates work, a second PC, config, development and tests. |
| [ARCHITECTURE.md](ARCHITECTURE.md) | The pieces, the decisions behind them, and an index of the sections in [architecture/](architecture/). |
| [MAP.md](MAP.md) | Where each thing lives, which doc section describes it and which test guards it. |
| [APP.md](APP.md) | The files of the Svelte app that the architecture sections do not cover. |
| [FOCUS.md](FOCUS.md) | Direction and stages: what counts as done, and what is left to test by hand. |
| [ROADMAP.md](ROADMAP.md) | Ideas worth keeping and the order they are planned in. |
| [FOUNDATIONS.md](FOUNDATIONS.md) | What tools of this kind are built on underneath, and how Hub compares. |
| [GARDEN.md](GARDEN.md) | A generated list of code that looks unused (`scripts/garden.py`). |
| [examples/](examples/) | `hub.toml` files that put another tool's status and actions in the app. |
| [design/](../design/DESIGN.md) | The design book: how the app looks and moves. |

## Conventions

- The first lines of every document say who it is for: `<!-- för: agent -->` or
  `<!-- för: Christoffer -->` ("för" is Swedish for "for").
- A section names the code it describes in a `<!-- covers: ... -->` line under its heading.
  `cargo test -p hub-core covers` fails when one points at a file that is gone.
- `cargo test -p hub-core --test it map::` checks that every path in `MAP.md` exists.
