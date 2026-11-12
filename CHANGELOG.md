# Changelog

All notable changes to this project are listed here. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the version numbers follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html). [README.md](README.md#versions) says what counts as a breaking change.

## [Unreleased]

## [0.2.0] - 2026-11-12

Help pages that teach. Tested against a simulated drive only, not against a real drive.

### Added

- The window shows the version in its title and in the top bar.
- This changelog, and the versioning rules in the README.
- Parameter table: a double-click on a row opens its help window. The up and down arrow keys move through the rows, and Enter opens the help window.
- A **View** menu shows or hides the factory parameters and the spare slots. Both are hidden by default. **Select in table** shows a hidden row when you open it from a link.
- A help page for every parameter that the table shows, 236 pages in all. Each page starts with "In plain words": an everyday picture of what the parameter does. Then come how it works, what a change does, and what it means on a CNC machine.
- New topic pages: how a servo drive works, parameters, the motor and drive, control modes, position, speed and torque control, inputs and outputs, vibration, tuning and protection. The front matter `group:` and `order:` set the topic for each parameter group and the reading order.
- The help window shows the parameter group, its topic and a link to the basics under the title.
- A test checks that every parameter that the table shows has a help page.

### Changed

- The older tuning pages now use the same teaching layout as the new pages.
- The help window no longer says that a parameter has no page.

## [0.1.0] - 2026-11-12

First public version. Tested against a simulated drive and real JMC 2.4.3 export files only, not against a real drive.

### Added

- `jmc` command-line tool with JSON output: ports, probe, params, info, read, snapshot, write, monitor, status, reset-alarm, raw-read, bench-read-size, import-xml, export-xml, diff, identify.
- `jmc-servo-tool` window: parameter table with baseline compare, checked RAM writes, drive status, live plots with CSV export.
- Safe writes: range checks, axis check against baselines, a backup first, read-back after each write. RAM by default; EEPROM only with `--save --yes`.
- This project's own parameter table for model 512 (`model512.toml`), with our own names and descriptions.
- Import and export of JMC 2.4.3 parameter files. JMC 2.4.3 loads the exported files.
- Help window with a page for every parameter, drawings from a loop model, links between pages, and topic pages for control loops, filters and gain sets.
- Simulated drive for every command and for the window.
- Snapshot file format `jmc-servo-tool/snapshot/1`.

[Unreleased]: https://github.com/bhowiebkr/JMC-Servo-Tool/compare/v0.2.0...HEAD
[0.2.0]: https://github.com/bhowiebkr/JMC-Servo-Tool/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/bhowiebkr/JMC-Servo-Tool/releases/tag/v0.1.0
