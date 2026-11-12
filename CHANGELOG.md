# Changelog

All notable changes to this project are listed here. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the version numbers follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html). [README.md](README.md#versions) says what counts as a breaking change.

## [Unreleased]

Fixes from a review of the tool, the parameter table and the help pages. Tested against a simulated drive only.

### Added

- `monitor` and the Live tab: new fields `pos_cmd_counts`, `pos_fb_counts` and `following_error_counts`, in encoder counts. The CSV export has the same columns.
- Write reports have a new field `failed`: the item that failed.
- `identify` has a new field `skipped`: the baselines that it could not compare with the drive, and why.
- `probe` and **Find settings** also try the framing 8O1.
- `identify` has a new field `factory_differing`: the number of parameters that differ from the factory values.

### Changed

- **Breaking:** `pos_cmd`, `pos_fb` and `following_error` now come from registers 2100 to 2105, in command pulses. This is the unit of P03-15 and of the steps that LinuxCNC sends. Before, they came from 2106 to 2111, in encoder counts, so a P03-15 value set from the Live tab was approximately 13 times too loose with the factory 10000 pulses for each turn. The monitor now reads 2100 to 2134. The units come from JMC's list of the monitor registers and are not yet checked on a drive.
- The Live tab shows the unit of each position value, and the plots say "command pulses".
- A write to a parameter that applies only after a power cycle now needs an EEPROM save (`--save --yes`). A power cycle clears RAM, so a RAM write did nothing.
- A value that goes to EEPROM is checked against the range, also when it does not change.
- `reset-alarm` and **Reset alarm** send the reset once. If the reply is lost, the tool reports an error and does not send a second reset. A second reset could clear a new alarm that you did not see.
- **Breaking:** the axis check refuses a write in two more cases. With no baseline of another axis that matches the drive less well, the check cannot tell the axes apart. If the drive is closer to the factory values than to its own baseline, the drive may have been reset. The check applies in `write` and in the window. Save a baseline of every axis before you write. The check also gives a warning for each baseline that it cannot use.
- **Breaking:** a write to a parameter that changes only with the servo off is refused while the drive reports the servo on. Before, the tool only marked these parameters. The servo-on bit is not yet checked on a real drive.
- If the table lists the values of a parameter, the range check accepts only those values. P00-27 now refuses a value above 7.

### Fixed

- A write with a value out of range no longer saves a backup first. The range check now comes before the backup, as the README says.
- When the drive reads back a different value after a write, the report now includes that value, and the window shows it in the table. After a write that failed in a different way, the window shows the value as unread until the next read.
- `write_many` refuses an address range past 65535. Before, the address could wrap around to 0.
- P03-15 help page: use the following error in command pulses, and check the unit with a known move first. The examples now use the factory value of 10000 pulses for each turn.
- P00-21 help page: JMC's PC tool gives 9600 bit/s as the factory value, but the drive manual gives 57600 bit/s. The **Find settings** button finds the real rate.
- P00-27 help page: only the values 0 to 7 select a rate. The table keeps JMC's range of 0 to 65535, and the tool refuses the other values.
- `abs_turns` and `abs_pos` in `monitor` and in the CSV export of the Live tab now follow P00-29, which swaps registers 2125 and 2126. Before, the tool always read the turns from 2125, but the factory value of P00-29 puts them in 2126. The tool reads P00-29 when the monitor starts.
- Tuning page, step 8: divide the following error by 10 before you set P03-15. The Live tab shows command pulses, and P03-15 counts in units of 10 pulses.

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
