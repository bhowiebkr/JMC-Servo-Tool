# Working on the JMC Servo Tool

[README.md](README.md) explains how to use the CLI and the window. If `AGENTS.local.md` exists, read it too: it holds the rules and paths of the local machine, and it is not in the repository.

## This drives a CNC machine

The drives move the axes of a CNC machine. Bad code or a bad setting can crash an axis, rack a gantry, ruin material or hurt someone.
- Don't change code when the user is only asking a question.
- **Never write to a real drive without the user's go-ahead for that write.** Reading is fine.
- **Never save to EEPROM (`--save`) without the user confirming it** in so many words. RAM writes are lost at power-off; EEPROM writes are not.
- Never loosen the write checks in `Drive::check_write`, the axis check in `guard::guarded_write`, or the window's `App::write_block` to make something easier. The window writes RAM only.
- The drive's serial settings (P00-21 to P00-26) are protected for a reason: changing them drops the connection, and the user has to fix it at the drive's keypad.

## Layout

| Path | What |
|---|---|
| `crates/jmc-core` | Library: Modbus RTU (`rtu`, `link`), parameter table (`params`), reads and safe writes (`drive`), live values and alarms (`monitor`), JMC export XML (`jmcxml`), snapshots, diffs and axis identification (`snapshot`), the checked write sequence used by every front end (`guard`), simulated drive (`sim`), help pages (`docs`), the loop model for the drawings (`model`), the text rules (`ste`) |
| `crates/jmc-core/data/model512.toml` | This project's parameter table for model 512: the facts of each register and our own text. It is the source; edit it by hand. `Schema::parse` checks it |
| `docs/parameters`, `docs/topics` | Help pages in Markdown, built into the program by `crates/jmc-core/build.rs`. Front matter: `see:`, optional `drawing:`, and `title:`, `group:` and `order:` for topics. Parameter pages use the sections "In plain words", "How it works" and "On your machine". Every parameter that the table shows needs a page (a test checks it) |
| `apps/jmc` | The `jmc` CLI. `tests/cli.rs` runs the binary against the simulator |
| `apps/jmc-servo-tool` | The window (eframe/egui). A worker thread owns the drive (`worker`); the window only sends commands and draws events. The help window is `more.rs`, its drawings `drawing.rs`, its Markdown `markdown.rs`. `src/tests.rs` draws frames headless against the simulator |

Planned: `jmc serve` (local API), `jmc-mcp`, and `jmc-cnc-agent` (Python, on the LinuxCNC PC).

## Build and check

```bash
cargo build
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

- Keep clippy clean and the tests passing.
- Rust 1.97.1, edition 2024, `unsafe_code = "deny"`.
- Set `JMC_EXPORTS_DIR` to a folder of JMC 2.4.3 exports to check that each one imports and exports again with the same values.
- Set `JMC512_XML` to JMC 2.4.3's model 512 parameter file to run the compatibility tests in `params/legacy.rs`.
- Neither file is in the repo.
- The window's tests never open a window; keep it that way (they run with `egui::Context::run_ui` and no renderer). The ignored test `more_window_pictures` draws off the screen with wgpu and saves PNG files to `JMC_SHOTS`.
- Test anything that touches the drive against `sim::SimDrive` first. It models RAM and EEPROM, power cycles, hidden parameters, refused writes and live values.

## Rules

- **GUI:** take Millwright's look only: its dark theme colours, Segoe UI / Consolas, and eframe/egui pinned at `=0.37.0`. Don't depend on `millwright-window` or other suite crates.
- **Every command prints JSON** for agents. Keep replies small; put large data in files and return the path.
- **JMC's files:** never add JMC's XML files, JMC's text or decompiled code to the repo. The facts (addresses, limits, scaling, choice values) go in `model512.toml` with our own words.
- **Text:** names, descriptions and help pages use Simplified Technical English. `jmc_core::ste` checks it (sentences of 25 words or fewer, no banned words, no CJK characters); the tests run it on the table and on every page.
- **Drawings:** colour carries meaning, one meaning per colour in all drawings, and never colour alone (add labels or line styles). Every model drawing says that it is a model.
- **Versions:** Semantic Versioning, with one version for all crates in `[workspace.package]` of the root `Cargo.toml`. Add each change to `## [Unreleased]` in `CHANGELOG.md`. Change the version only for a release; the README section "Versions" says what counts as breaking. Tag releases `vX.Y.Z`.
- **Publishing:** the repo is public. Push only with the user's go-ahead.
