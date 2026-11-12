---
title: Tuning step by step
order: 110
see: P01-02 P01-03 P01-04 P02-03 P08-11 topic:control-loops
---
Tuning means setting the gains so that the axis follows the command closely, without noise or vibration. This page gives a safe order of work for a CNC axis in position mode.

## Before you start

- Save a snapshot of all the parameters. Then you can go back at any time.
- Check that the axis can move freely and that the limit switches work.
- Keep a hand near the emergency stop.
- Change one setting at a time.

## The steps

1. Use auto-tuning. Check that P01-02 is 1, the standard mode. This is the factory setting. The stiffness level P01-03 then sets most of the gains.
2. Set the inertia ratio. Run the inertia identification, or calculate the ratio. Set the result in P01-04. See [Inertia and vibration](topic:vibration).
3. Increase the stiffness. Raise P01-03 by one level at a time, and move the axis after each change. Stop when the motor starts to hum, buzz or vibrate. Then go back 2 or 3 levels, so that you keep a margin.
4. Watch the following error. Run your fastest normal move and watch the following error on the Live tab. A smaller following error gives more accurate cuts.
5. Add feedforward. Increase P02-03 in steps of 10 %. The following error during moves becomes smaller. Stop if the axis overshoots at the end of a move.
6. Remove a resonance. If one frequency causes the noise, use a notch filter or the adaptive notch P08-11. Then you can often raise the stiffness by a few more levels.
7. Tune by hand only if necessary. Set P01-02 to 0 and adjust the gains yourself. See [Control loops](topic:control-loops) and [Gain sets](topic:gain-sets).
8. Set the following error alarm. When the tuning is good, set P03-15 to 2 to 3 times the largest following error that you saw. See [Protection and alarms](topic:protection).
9. Save. When you are happy, save the values to EEPROM with the command-line tool. Until then, a power cycle brings back the old values.

## Signs to look for

| What you see or hear | Probable cause | What to try |
|---|---|---|
| A high whine at a stop | The gains are too high, or the speed signal is noisy | Lower P01-03, or increase P08-19 or P08-20 a little |
| A low hum or growl | A mechanical resonance | Use a notch filter or the adaptive notch |
| Overshoot at the end of a move | Too much feedforward, or a wrong inertia ratio | Lower P02-03, or check P01-04 |
| A soft response and a large following error | The gains are too low, or the inertia ratio is too low | Raise P01-03, or check P01-04 |
| AL.501 during fast moves | The axis cannot follow, or P03-15 is too small | Lower the acceleration in LinuxCNC, or check the torque limit |
