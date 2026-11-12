---
title: Control loops
see: P02-00 P02-10 P02-03 P01-04 topic:filters
---
The drive moves the motor with three control loops. Each loop is inside the next one.

| Loop | Input | Output | Main parameters |
|---|---|---|---|
| Position loop | Following error | Speed command | P02-00, P02-03 |
| Speed loop | Speed error | Torque command | P02-10, P02-11, P02-12 |
| Current loop | Torque command | Motor current | Set at the factory |

## How the loops work together

1. The position loop compares the commanded position with the encoder position. The difference is the following error.
2. The position loop multiplies the following error by the position loop gain (P02-00). The result is the speed command.
3. The speed loop compares the speed command with the measured speed. It makes a torque command from the difference.
4. The current loop makes the motor current that gives this torque.

Each loop must be faster than the loop around it. Set the speed loop first. Then set the position loop.

## Following error

At a constant speed, the following error is equal to the speed divided by the position loop gain. At 1000 rpm and a gain of 48.0 1/s, the error is approximately 0.35 motor turns.

Speed feedforward (P02-03) sends a part of the commanded speed directly to the speed loop. Then the position loop must correct less, and the following error becomes smaller.

## Inertia ratio

The speed loop gain is a true bandwidth in Hz only when the load inertia ratio (P01-04) is correct. The real gain is the set gain multiplied by (1 + set ratio) ÷ (1 + real ratio).

- If P01-04 is too low, the real speed loop gain is lower than the set value. The motor feels soft.
- If P01-04 is too high, the real gain is higher than the set value. The motor can vibrate or make noise.

## Filters

Filters in the loops remove noise and resonance, but each filter adds a delay. A loop with more delay becomes unstable at a lower gain. See [Filters](topic:filters).
