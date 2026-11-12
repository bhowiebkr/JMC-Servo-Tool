---
title: Gain sets
see: P02-30 P02-00 P02-01 P01-02
---
The drive keeps two sets of loop gains. It can change between them while it runs. For example, it can use one set during moves and a different set at a stop.

| Gain | Set 1 | Set 2 |
|---|---|---|
| Position loop gain | P02-00 | P02-01 |
| Speed loop gain | P02-10 | P02-13 |
| Speed loop integral time | P02-11 | P02-14 |
| PDFF factor | P02-12 | P02-15 |

The two sets use the same values for the other loop parameters. Examples are the feedforward (P02-03), the filters (P08-19, P08-20) and the inertia ratio (P01-04).

## When the drive uses gain set 2

The gain switching mode (P02-30) selects the condition. When the condition is true, the drive uses gain set 2. When the condition stops, the drive waits for the delay P02-33. Then it goes back to gain set 1.

| Value of P02-30 | Gain set 2 is active when |
|---|---|
| 0 | Never. Gain set 1 only. |
| 1 | Always. Gain set 2 only. |
| 2 | A digital input with function 9 is on. |
| 3 | The torque command is above the level. |
| 4 | The change of the speed command is above the level. |
| 5 | The speed command is above the level. |
| 6 | The position error is above the level. |
| 7 | A position command comes in. |
| 8 | Positioning is not complete. |
| 9 | The actual speed is above the level. |
| 10 | A position command comes in, or the actual speed is above the level. |

The level is P02-31. The hysteresis is P02-32.

## The changeover

- The position loop gain changes smoothly from P02-00 to P02-01. The time for this change is P02-34.
- The return to gain set 1 waits for the delay P02-33 after the condition stops.

## Factory settings

The factory mode is 7. So gain set 2 is active during moves, and gain set 1 is active at a stop.

- Gain set 2 has a higher position gain (57.0 1/s) and almost no integral action (P02-14 = 1000.0 ms).
- Gain set 1 has a strong integral action (P02-11 = 10.0 ms). It holds the position firmly at a stop.

In the auto-tuning modes 1 to 3 (P01-02), the drive sets most of these gains from the stiffness level (P01-03).

## Tips

- To tune by hand, start with P02-30 = 0. Then only gain set 1 is active, and each change has one clear effect.
- When gain set 1 is good, copy it to gain set 2. Then set P02-30, and adjust gain set 2 for the moves.
