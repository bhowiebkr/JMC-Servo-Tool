---
title: Speed control
group: 4
order: 70
see: P04-05 P04-06 P04-00 topic:control-modes
---
Group P04 has two jobs. In speed mode, it selects where the speed command comes from. In every mode, it sets the overspeed alarm and the speed signals.

## Speed protection

P04-05 is the overspeed alarm. If the motor turns faster than this value, the drive stops with AL.420. This works in every control mode.

A correct position command never asks for such a speed. So the alarm catches a fault, for example a noisy step signal or a motor that runs out of control. Set P04-05 a little above the fastest speed that your machine uses. Then a fault stops the machine before it gets to a dangerous speed.

P04-06 and P04-07 limit the speed in each direction. The manual lists them with the speed mode settings. It does not say if they also apply in position mode.

## Speed signals

| Setting | The drive sets this signal when |
|---|---|
| P04-10 | The speed is below this value: zero speed. |
| P04-11 | The speed is above this value: the motor turns. |
| P04-12 | The speed is within this range of the command: speed match. |

A digital output can send these signals to a controller. See [Inputs and outputs](topic:inputs-outputs).

## Speed mode

In speed mode (P01-01 = 1), the command is a speed, not a position. The drive turns the motor at this speed and does not count the position. A spindle often works in this way.

P04-00 selects the source of the speed command.

| Value | Source |
|---|---|
| 0 | An analog voltage, −10 V to +10 V. The scale is P06-40. |
| 1 | A fixed speed in P04-02 |
| 2 | A field bus |
| 3 | Eight internal speeds, P04-30 to P04-37. Three digital inputs select one of them. |

- P04-01 reverses the analog command.
- P04-03 and P04-04 hold the motor still when the speed command is almost zero. An analog signal is never exactly 0 V, so without this clamp the motor can creep slowly.
- P04-14 and P04-15 set ramps, so that the speed changes smoothly. They apply in speed mode only. In position mode, the controller plans the ramps.
