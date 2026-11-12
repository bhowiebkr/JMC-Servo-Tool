---
title: Inertia and vibration
group: 8
order: 100
see: P08-01 P01-04 P08-11 P08-20 topic:filters topic:tuning
---
Group P08 helps the drive cope with a real machine. A real machine has mass, springiness and noise. These settings measure the mass and remove the vibration.

## Inertia identification

The drive needs the load inertia ratio P01-04 to set its loops. The drive can measure this ratio. It moves the motor back and forth and measures how much torque each change of speed needs. Then it calculates the ratio.

| Setting | Meaning |
|---|---|
| P08-01 | Turns the identification on (0) or off (1) |
| P08-02 | The maximum speed of the test moves |
| P08-03 | The ramp time of the test moves |
| P08-04 | The wait after each test move |
| P08-05 | The turns in each test move, calculated by the drive |

The test moves the axis. Make sure that the axis has free space in both directions before you start.

## Resonance

Hold a ruler on the edge of a table and flick the free end. It vibrates at one frequency. Every part of a machine does the same: the gantry, the belts, the couplings and the ball screw.

If the drive pushes at this frequency, the vibration grows. You hear a hum or a whine, and you can see marks on the cut surface. Resonance limits how stiff you can make the loops.

## Filters

Filters remove the parts of a signal that cause trouble.

- The low-pass filters P08-19, P08-20 and P08-21 remove fast noise. They make the motor quieter, but they add a delay.
- The notch filters P08-30 to P08-41 remove one frequency. They remove a resonance and leave the rest of the signal.
- The adaptive notch P08-11 finds a resonance by itself and sets notch 3 and notch 4. P08-13 sets how sensitive it is.

See [Filters](topic:filters) for the details.

## Disturbance observer

A cutting force pushes the axis away from its position. The loops correct this, but only after the error starts. The disturbance observer estimates the force earlier, from the motor current and the speed. Then it adds a torque that cancels the force. P08-25 sets how strong this correction is, and P08-26 filters it.

Start at 0 and increase it in small steps. Too much correction makes noise.
