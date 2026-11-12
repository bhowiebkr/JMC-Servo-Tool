---
title: Control loops
group: 2
order: 50
see: P02-00 P02-10 P02-03 P01-04 topic:gain-sets topic:filters topic:tuning
---
Group P02 holds the gains of the control loops. A gain says how hard the drive reacts to an error. This page explains the three loops and what each gain does.

## In plain words

Think of driving a car to a parking spot. Your eyes measure the distance to the spot. Your foot on the pedal sets the speed. The engine makes the force. In a servo drive, these are the position loop, the speed loop and the current loop.

A good driver reacts firmly but calmly. A nervous driver brakes and accelerates too hard, and the car jerks. A sleepy driver reacts late and stops in the wrong place. The gains set what kind of driver the drive is.

## The three loops

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

Each loop must be faster than the loop around it. The current loop is the fastest. If you tune by hand, set the speed loop first. Then set the position loop.

## What each gain does

- **Position loop gain** (P02-00): how much speed the drive commands for each unit of following error. A higher gain gives a smaller following error and a stiffer axis. Too high a gain gives overshoot.
- **Speed loop gain** (P02-10): how much torque the drive commands for each unit of speed error. A higher gain gives an axis that resists cutting forces better. Too high a gain gives a buzz or a whine.
- **Speed loop integral time** (P02-11): how fast the drive removes a small error that stays. Friction or gravity can cause such an error. A shorter time removes it faster. Too short a time gives a slow wobble.
- **PDFF factor** (P02-12): the mix of two methods of speed control. See its page for the details.

## Following error at a constant speed

At a constant speed, the following error is equal to the speed divided by the position loop gain. For example, 1000 rpm is 16.7 turns each second. With a gain of 48.0 1/s, the error is 16.7 ÷ 48.0 = approximately 0.35 motor turns.

Speed feedforward (P02-03) sends a part of the commanded speed directly to the speed loop. Then the position loop must correct less, and the following error becomes smaller. With 50 % feedforward, the error at a constant speed becomes approximately half.

Torque feedforward (P02-19) does the same for acceleration. It sends the torque that an acceleration needs before an error starts.

## Inertia ratio

The speed loop gain is a true bandwidth in Hz only when the load inertia ratio (P01-04) is correct. The real gain is the set gain multiplied by (1 + set ratio) ÷ (1 + real ratio).

- If P01-04 is too low, the real speed loop gain is lower than the set value. The motor feels soft.
- If P01-04 is too high, the real gain is higher than the set value. The motor can vibrate or make noise.

## Auto-tuning and gain sets

In the auto-tuning modes 1 to 3 (P01-02), the drive sets most of these gains from the stiffness level P01-03. To tune by hand, set P01-02 to 0.

The drive keeps two sets of gains and can change between them. See [Gain sets](topic:gain-sets).

## Other settings in this group

- P02-16 limits the integrator of the speed loop.
- P02-40 and P02-41 change the speed loop from PI control to P control under a condition.
- P02-50 adds a fixed torque, for example to hold a vertical axis against gravity.
- P02-51 and P02-52 add a torque to overcome friction.
- P02-59 selects the format of the current loop gains.

## Filters

Filters in the loops remove noise and resonance, but each filter adds a delay. A loop with more delay becomes unstable at a lower gain. See [Filters](topic:filters).
