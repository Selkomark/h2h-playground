# The physics of the slingshot, in plain language

No calculus here. We start with what you see on screen and build up from there.
Press `m` in the game to see these formulas with your own shot's numbers in them.

## 1. What you see on screen

- A slingshot with a small **`+`** in its fork. This is the **launch point**.
- While you aim, a pouch pulled back from the fork, and a short **dotted line**
  in front of it: the first 0.7 seconds of the path the ball would take.
- When you throw, the ball flies along a curve and leaves a **trail**.

**Where the ball really starts.** The pouch is drawn pulled back, but in this
game the flight starts exactly at the `+`, which is 3 m above the ground. The
dots, the formulas and the simulation all start there. (A real slingshot lets
go behind the fork. We simplify, so there is one clear starting point.)

All positions below are measured **from the `+`** and describe the **centre** of
the ball:

- `x` = how far **right** of the `+` the ball is.
- `y` = how far **above** the `+`. A negative `y` means below it.

So the height above the ground is `3 + y`. The ball has a radius of 0.45 m, so it
touches flat ground when its centre is 0.45 m up, which is `y = -2.55`.

## 2. Symbols and units

| Symbol | Meaning | Unit |
|---|---|---|
| `m` | metre, a unit of length (in formulas, it follows a number) | |
| `s` | second, a unit of time (it follows a number) | |
| `a` | launch angle: how steeply you throw. 0° is flat, 90° is straight up | degrees (°) |
| `v` | launch speed: how fast the ball leaves the `+` | metres per second (m/s) |
| `vx` | the sideways part of the launch speed | m/s |
| `vy` | the upward part of the launch speed | m/s |
| `t` | time since the throw | seconds (s) |
| `x(t)` | sideways distance at time `t`. Read it "x at time t". It does **not** mean x times t | m |
| `y(t)` | height above the `+` at time `t` | m |
| `g` | gravity, 9.81: each second, gravity takes 9.81 m/s off the upward speed | m/s per second, written m/s² |
| `T` | time to come back down to the height of the `+` | s |
| `H` | greatest height above the `+` | m |
| `R` | sideways distance when back at the height of the `+` | m |

`t²` means `t × t`. Two things written next to each other are multiplied:
`vx t` means `vx × t`, and `2 vy` means `2 × vy`.

## 3. From pulling to throwing

You choose two things by pulling:

- **How far** you pull sets the speed: every metre of pull gives 5 m/s.
  Pulling more than 5 m adds nothing, so the top speed is 25 m/s.
- **Which way** you pull sets the angle. The ball goes the opposite way: pull
  down and left to throw up and right.

The angle stays between 0° and 90°. Releases below 2 m/s do not throw.

"5 m/s per metre" is a rule of this game. (An ideal spring is similar: pull
twice as far and the object leaves about twice as fast.)

Code: `pull_to_shot` in `src/physics.rs`.

## 4. Sine and cosine, from scratch

Your throw has a speed and a direction. It helps to split it into two separate
speeds: one **sideways** and one **upward**.

Draw the throw as a slanted arrow. Then draw the flat and upright sides that
make it into a triangle with a square corner:

```text
                              *
                            / |
                          /   |
          arrow         /     |
       (length 1)     /       |  upright side = sin(a)
                    /         |
                  /           |
                /  a        __|
              *______________|_|

                 flat side = cos(a)
```

For an arrow of length **1** leaning at angle `a`:

- **cos(a)** (cosine) is the length of the **flat** side.
- **sin(a)** (sine) is the length of the **upright** side.

That is all they are: two lengths that tell you how much of a slanted arrow
goes across and how much goes up. Your calculator knows them (set it to
**degrees**).

| Angle `a` | cos(a), across | sin(a), up | In words |
|---|---|---|---|
| 0° | 1.000 | 0.000 | all across |
| 30° | 0.866 | 0.500 | mostly across |
| 45° | 0.707 | 0.707 | equal shares |
| 60° | 0.500 | 0.866 | mostly up |
| 90° | 0.000 | 1.000 | all up |

The two shares do not add up to 1. They are sides of a triangle, and the slanted
side is shorter than the other two sides put end to end.

If the arrow is `v` long instead of 1, the whole triangle is `v` times bigger:

> **vx = v cos(a)**  **vy = v sin(a)**

**Example.** Throw at `v = 10 m/s` and `a = 30°`:

- `vx = 10 × 0.866 = 8.66 m/s`
- `vy = 10 × 0.500 = 5.00 m/s`

Code: `velocity_components` in `src/physics.rs`.

## 5. The big idea: two motions at once

Once the ball has left the slingshot, and until it touches something, the only
thing acting on it in this game is gravity. Gravity pulls **straight down**. So:

- **Sideways**, nothing changes. The ball keeps its speed `vx`.
- **Up and down**, the speed changes steadily: it loses 9.81 m/s every second.

The two do not affect each other. You can work them out separately. The ball's
weight does not appear anywhere: in free flight a heavy ball and a light ball
thrown the same way follow the same path. (Weight matters a lot when the ball
**hits** something.)

## 6. Sideways: x(t)

At a steady speed, distance is speed times time:

> **x(t) = vx t**

**Example** (10 m/s at 30°): after 0.5 s, `x = 8.66 × 0.5 = 4.33 m`.

Code: `x_at` in `src/physics.rs`.

## 7. Up and down: y(t)

The upward speed starts at `vy` and drops by `g` each second. After `t` seconds
it is:

```text
upward speed now = vy - g t
```

For `vy = 5`: at the start 5.00 m/s, after 0.5 s only 0.10 m/s (almost stopped
rising), and after 1 s it is -4.81 m/s. A minus sign means it is going **down**.

How far has it risen? The speed keeps changing, but it changes **evenly**. When
something changes evenly, its average is halfway between the first value and the
last one:

```text
average upward speed = ( vy  +  (vy - g t) ) / 2
                     =   vy - g t / 2
```

Height is average speed times time. Multiply by `t`:

> **y(t) = vy t - g t² / 2**

Read it as two pieces. `vy t` is how high the ball would get with no gravity.
`g t² / 2` is how far gravity has pulled it below that. Because of `t × t`, the
second piece grows faster and faster, and that is why the path bends into a
curve.

**Example** (`vy = 5`): after 0.5 s,
`y = 5 × 0.5 - 9.81 × 0.25 / 2 = 2.50 - 1.23 = 1.27 m` above the `+`.

Code: `y_at` in `src/physics.rs`. `position_at` combines `x_at` and `y_at`; the
dotted preview is made with it (`preview_points` in `src/render.rs`).

## 8. Flight time: T

**Important:** `T` is the time to come back down **to the height of the `+`**,
assuming nothing is in the way. It is not the time to hit the ground, which is
lower.

Being back at that height means `y = 0`. From section 7, height is the average
upward speed times time:

```text
y = (vy - g t / 2) × t
```

A multiplication gives zero when one of its parts is zero. Either `t = 0` (the
start) or `vy - g t / 2 = 0`. The second one gives `t = 2 vy / g`:

> **T = 2 vy / g**

Another way to see it: it takes `vy / g` seconds to lose all the upward speed,
and just as long to fall back down.

**Example:** `T = 2 × 5 / 9.81 = 1.02 s`. Check it with `y(t)` using the
unrounded `T`: you get exactly 0.

Code: `flight_time` in `src/physics.rs`.

## 9. Maximum height: H

The ball is at the top when it stops rising, which is when `vy - g t = 0`, so
`t = vy / g`. That is exactly half of `T`.

On the way up, the speed falls evenly from `vy` to 0, so the average is
`vy / 2`. Multiply by the time `vy / g`:

> **H = vy² / (2 g)**

`H` is measured **above the `+`**. Above the ground it is `3 + H`. It assumes
the ball hits nothing on the way up.

**Example:** `H = 5² / (2 × 9.81) = 25 / 19.62 = 1.27 m`, or 4.27 m above the
ground.

Because `vy` is squared, doubling the upward speed makes the ball go **four**
times as high.

Code: `max_height` in `src/physics.rs`.

## 10. Range: R

**Important:** like `T`, `R` assumes the ball **lands at the height of the `+`**
with nothing in the way.

The ball moves sideways at `vx` for a time `T`:

```text
R = vx × T = v cos(a) × 2 v sin(a) / g = v² × (2 sin(a) cos(a)) / g
```

There is a neat shortcut: `2 sin(a) cos(a)` is the same number as `sin(2a)`, the
sine of **double** the angle. So:

> **R = v² sin(2a) / g**

**Why the shortcut is true.** Take two sticks of length 1 and open them to an
angle of `2a`. Join the ends to make a triangle. Its area can be found in two
ways.

```text
   Way 1: one stick is the base       Way 2: cut down the middle

            * top                                  *
           /: \                                  / |
        1 / :   \                             1 /  | sin(a)
         /  :     \                            /   |
        /   : <---- \ height = sin(2a)        / a  |
       /2a  :         \                      *-----+  cos(a) along the cut
      *---------------*                       \ a  |
              1                                \   |
                                              1 \  | sin(a)
                                                 \ |
                                                   *
```

- Way 1: area = half × base × height = `½ × 1 × sin(2a)`.
- Way 2: two identical square-cornered triangles, each
  `½ × cos(a) × sin(a)`, so together `sin(a) cos(a)`.

It is the same shape, so `½ sin(2a) = sin(a) cos(a)`. Double both sides and you
have the shortcut. (Check with a calculator: at `a = 30°`,
`2 × 0.5 × 0.866 = 0.866`, and `sin(60°) = 0.866`.)

If `2a` is more than 90°, the "height" in Way 1 leans back over the other way,
but it is still the sine: sine rises from 0 to 1 as the angle goes from 0° to
90°, then falls back to 0 at 180° in a mirror image. So `sin(120°) = sin(60°)`.

**Example:** `R = 10² × sin(60°) / 9.81 = 100 × 0.866 / 9.81 = 8.83 m`.
The same as `vx × T = 8.66 × 1.019`.

Code: `level_range` in `src/physics.rs`.

### Two things the range formula tells you

For a fixed speed, only `sin(2a)` changes with the angle.

- **45° goes farthest.** Sine is never bigger than 1, and it equals 1 at 90°.
  That happens when `2a = 90°`, so `a = 45°`.
- **30° and 60° go equally far.** Double them: 60° and 120°. Those have the
  same sine. The same is true for any pair that adds up to 90°, such as 20° and
  70°. The steep one goes much higher and takes longer.

Both statements are about **landing at launch height**.

## 11. One full worked example

`v = 14 m/s`, `a = 45°`:

| Step | Calculation | Result |
|---|---|---|
| `vx` | 14 × 0.707 | 9.90 m/s |
| `vy` | 14 × 0.707 | 9.90 m/s |
| `x(1)` | 9.90 × 1 | 9.90 m |
| `y(1)` | 9.90 × 1 - 9.81 × 1² / 2 | 4.99 m |
| `T` | 2 × 9.90 / 9.81 | 2.02 s |
| `H` | 9.90² / (2 × 9.81) | 5.00 m above the `+` (8.00 m above ground) |
| `R` | 14² × sin(90°) / 9.81 | 19.98 m |

Small differences in the last digit come from rounding along the way.

## 12. The ground is lower than the launch point

After 2.02 s and 19.98 m, that ball is level with the `+` again, still 3 m up.
With open ground it keeps falling. It touches the ground when `y = -2.55`
(section 1). You can find that moment by trying times in `y(t)`:

| `t` | `y(t)` for 14 m/s at 45° |
|---|---|
| 2.02 s | about 0.00 m |
| 2.20 s | -1.96 m |
| 2.25 s | -2.56 m (touchdown) |

By then `x = 9.90 × 2.25 = 22.3 m`, well past `R`.

This also changes the best angle **to the ground**. At 14 m/s from this
slingshot, about **42°** reaches slightly farther along the ground than 45°. The
extra height gives the ball extra falling time, so it pays to throw a bit
flatter and move sideways faster.

## 13. After the first hit

The formulas describe free flight only. Once the ball touches the ground or an
object, bouncing, spinning, sliding and toppling are handled by the physics
engine (rapier2d). The panel says when this happens.

The game also moves time forward in small steps (120 per second) instead of
perfectly smoothly, so before the first hit its ball can differ from the
formulas by a few **centimetres**. You will rarely see a difference. The code
for the stepping is in `src/world.rs`.

## 14. Things to try

Press `m` first. The dashed line is the height of the `+`; compare where the
trail crosses it with `R`. Use the arrow keys for exact values (1° and
0.25 m/s per press). Solid objects get in the way, so use shots that fly high
over them or fall short of them.

1. **Equal ranges.** Throw at 12 m/s at 30°, then at 60°. Both should cross the
   dashed line about 12.7 m from the `+`. Which one takes longer?
2. **Best angle.** At 12 m/s, try 35°, 45° and 55°. Work out `R` for each first.
3. **Four times the height.** At 90°, compare `H` at 8 m/s and 16 m/s.
4. **Double speed.** At 45°, compare `R` at 8 m/s and 16 m/s. Why is it four
   times as far, not twice?
5. **Flat throw.** At 0°, `vy = 0`, so `T`, `H` and `R` are all 0: the ball is
   at launch height only at the start. It still flies forward while it falls.
6. **Predict, then fire.** Pick a time, work out `x(t)` and `y(t)` on paper,
   then watch the panel during the flight.
7. **Same fall.** Throw flat at 8 m/s and at 12 m/s. Both take about 0.72 s to
   reach the ground, if they hit nothing first. Only the distance differs.

## 15. The real world has air

This game has **no air drag** and no wind. Real air pushes against a moving
object and slows it down. A real thrown object usually **falls short** of these
predictions: it does not go as high or as far. The effect is bigger for fast,
light or wide things (a beach ball, a shuttlecock) and smaller for slow, heavy,
small ones (a steel ball). With drag, the best angle for distance is usually
lower than the no-air answer. Spin can also bend a real path.
