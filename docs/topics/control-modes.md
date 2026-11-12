---
title: Control mode and brakes
group: 1
order: 40
see: P01-01 P01-02 P01-10 P01-30 topic:protection
---
Group P01 holds the basic choices: what the drive controls, how it tunes itself and how it stops the motor.

## Control mode: what does the drive control?

A drive can follow three kinds of command.

| Mode | The command says | Typical use |
|---|---|---|
| Position | "Go to this position." | CNC axes with step and direction |
| Speed | "Turn at this speed." | Spindles and conveyors |
| Torque | "Push with this force." | Winders, presses and tension control |

P01-01 selects the mode. A CNC machine with step and direction uses position mode, which is the factory setting 0. In position mode, the drive also controls the speed and the torque, but only as steps on the way to the position.

The mixed modes 3 to 5 let a digital input change between two modes while the machine runs.

## Auto-tuning: let the drive set the gains

A servo has many gains. Auto-tuning (P01-02) sets most of them from one number, the stiffness level P01-03. So you turn one knob instead of ten.

For good auto-tuning, the drive must also know the load inertia ratio P01-04. See [Tuning step by step](topic:tuning).

## Overtravel: the limit switches

A limit switch at each end of an axis stops the axis before it hits the end. When a limit input operates, the drive stops the motor. After that, it accepts only moves away from the limit. P01-10 selects if the motor holds its position or coasts.

Many LinuxCNC machines connect the limit switches to LinuxCNC, not to the drive. Then the limit inputs of the drive are not used.

## Dynamic brake

At servo off, the drive normally lets the motor coast. A heavy axis can coast a long way.

A dynamic brake stops the motor faster. It connects the motor windings together. The turning motor then works as a generator into a short circuit, and this makes a strong braking force. A bicycle dynamo feels harder to turn when the lamp is on for the same reason.

P01-21 to P01-24 select when the drive uses the dynamic brake. Note that 0 means "use", and 1 means "do not use". The factory setting is 1 for all four. P01-20 sets the delay before the brake operates.

The dynamic brake slows the motor, but it does not hold it. At a very low speed, the braking force goes to zero.

## Holding brake

A vertical axis, for example a Z axis, falls when the motor has no current. A holding brake is a mechanical brake inside the motor. A spring applies it, and current releases it. So the brake holds the axis when the power is off.

The drive controls the brake through a digital output with function 2. The timing is important.

- At servo on, the drive energizes the motor and releases the brake. It accepts commands after the time P01-30.
- At servo off with the motor stopped, the drive applies the brake. It keeps the motor energized for the time P01-30, so the axis does not drop while the brake closes.
- At servo off with the motor turning, the drive waits until the speed is below P01-31, or until the time P01-32 ends. Then it applies the brake.

A holding brake is not for stopping a moving axis. It wears quickly if it stops motion often.

## Other settings

- P01-35 sets the width of the Z pulse on the encoder output.
- P01-40 detects a motor that runs out of control. Keep it on.
