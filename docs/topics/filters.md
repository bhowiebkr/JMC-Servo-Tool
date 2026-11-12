---
title: Filters
see: topic:control-loops P08-20 P08-30 P08-11
---
The drive has two types of filter. A low-pass filter removes fast changes. A notch filter removes one frequency.

## Low-pass filters

A first-order low-pass filter has a time constant T. After a step, the output gets to 63 % of the step after one time constant. It gets to 95 % after three time constants.

The corner frequency is 1 ÷ (2π × T). Above the corner frequency, the filter decreases the signal more and more.

| Parameter | What it filters |
|---|---|
| P08-19 | Speed feedback |
| P08-20 | Torque command |
| P08-21 | Torque command at low speed |
| P02-04 | Speed feedforward |
| P02-20 | Torque feedforward |
| P03-16 | Position command |
| P08-26 | Disturbance observer |

A longer time constant gives less noise and a softer sound. But it adds delay in the loop. Then the speed loop gain (P02-10) must be lower.

## Notch filters

A notch filter removes a narrow band of frequencies around its center frequency. Use it when the machine has a mechanical resonance at one frequency. The drive has four notch filters.

| Set | Frequency | Width | Depth |
|---|---|---|---|
| 1 | P08-30 | P08-31 | P08-32 |
| 2 | P08-33 | P08-34 | P08-35 |
| 3 | P08-36 | P08-37 | P08-38 |
| 4 | P08-39 | P08-40 | P08-41 |

- The frequency is the center of the notch, in Hz. A frequency of 5000 Hz turns the notch off.
- The width grade sets how wide the notch is. A larger grade gives a wider notch.
- The depth grade is the output divided by the input at the center, in %. A depth of 0 removes the center frequency fully. A larger depth removes less.

The adaptive notch (P08-11) can find a resonance and set notch 3 and notch 4 automatically.
