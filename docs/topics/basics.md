---
title: How a servo drive works
order: 10
see: topic:parameters topic:control-loops topic:tuning topic:protection
---
Start here if servo drives are new to you. This page explains the parts of a servo system and how they work together. The other help pages build on these ideas.

## The parts of the system

| Part | What it does |
|---|---|
| Motion controller | Plans each move and sends the commands. On a CNC machine, this is usually LinuxCNC or a similar controller. |
| Servo drive | Receives the commands and sends current to the motor. It watches the encoder all the time. |
| Servo motor | Turns the current into torque. The torque turns the ball screw or the belt. |
| Encoder | A sensor on the back of the motor. It tells the drive the exact angle of the shaft. |

The encoder on these motors counts 131072 steps in one turn. So the drive always knows the angle of the shaft to less than 0.003°.

## Servo or stepper?

A stepper motor moves one step for each pulse. It has no sensor. If the load is too heavy, the stepper loses steps, and the controller does not know about it.

A servo motor is different. The drive compares the commanded position with the encoder position thousands of times each second. If the motor falls behind, the drive increases the current to catch up. If the motor cannot catch up, the drive stops it and shows an alarm. So a servo does not lose its position without a warning.

## Step and direction

LinuxCNC tells the drive where to go with two signals. Each pulse on the step signal is one small move. The direction signal tells the drive which way to go.

With the factory setting of P03-09, 10000 pulses make one motor turn. So 10000 pulses each second give a speed of 60 rpm. The pulse count sets the distance, and the pulse rate sets the speed. See [Position control](topic:position-control).

## Following error: the gap

The motor is always a little behind the command. The distance between the commanded position and the real position is the following error.

Think of a dog on a leash. You walk, and the dog follows. At a steady pace, the leash has a small, constant slack. When you start or stop quickly, the leash goes tight for a moment.

- A small following error means that the machine cuts the shape that you programmed.
- A large following error during a move rounds the corners and changes the size of curves.
- A very large following error means that something is wrong. The drive then stops with an alarm. The limit is P03-15.

You can see the following error on the Live tab.

## How the drive closes the gap

The drive works like the cruise control in a car, but it does three jobs at the same time. Each job is a control loop.

1. The position loop looks at the gap. It thinks: "I am behind. I must go faster." Its output is a speed command.
2. The speed loop compares this speed command with the real speed. Its output is a torque command.
3. The current loop sends the current that makes this torque.

This happens thousands of times each second. The gains of the loops decide how hard the drive pushes for a given error. See [Control loops](topic:control-loops).

## Stiffness: the spring picture

Think of the drive as an invisible spring between the commanded position and the motor shaft.

- A stiff spring pulls the motor back quickly. The following error stays small, and a cutting force cannot push the axis away.
- A soft spring lets the motor lag and wander. The motion is smooth, but less accurate.
- A spring that is too stiff makes the axis buzz, hum or shake. The loops correct so hard that they overshoot again and again.

The stiffness level P01-03 sets the strength of this spring. Tuning is the work of finding the stiffest setting that stays quiet. See [Tuning step by step](topic:tuning).

## Inertia: how heavy the load feels

Inertia is the resistance of a mass to a change of speed. A heavy gantry needs much more torque to accelerate than the bare motor.

The drive must know how heavy the load is to set its loops correctly. P01-04 tells the drive the load inertia compared with the inertia of the motor rotor. If this value is wrong, the drive pushes too hard or too gently. See [Inertia and vibration](topic:vibration).

## Servo on and servo off

At servo off, the drive sends no current. You can turn the motor shaft by hand, and the motor does not hold its position.

At servo on, the drive energizes the motor and holds the position. LinuxCNC usually controls the servo on input with its enable signal. See [Inputs and outputs](topic:inputs-outputs).

## Alarms

When the drive finds a problem, it stops the motor and shows an alarm code, for example AL.501. An alarm output can tell LinuxCNC to stop the whole machine. See [Protection and alarms](topic:protection).

## Where the settings live

The drive keeps several hundred settings, called parameters. Each setting has a code like P03-15. See [Parameters](topic:parameters) to learn how to read the codes and how to change them safely.
