# sol-animation

SOL's unified animation engine. Animation is not decoration; it is part of
SOL's interaction model (PRD §4.4 Interactive Motion).

## Model

```text
Current State
      ↓
Animation
      ↓
Target State
```

Supports: easing / spring physics / interactive progress / velocity /
interruption / reversal.

Semantic motion tiers (PRD §19 Motion tokens):

```text
Motion::Fast
Motion::Control
Motion::Panel
Motion::Window
Motion::Workspace
```

Apps must not name a millisecond duration; they name an **animation
semantic role**, and duration + curve resolve from `sol-design` tokens.

## Status

**Phase 2 semantic-motion foundation implemented.** Motion specifications,
tiers, animation-driver contracts, and interruptible-animation state are
present. The compositor animation system (gestures / workspace motion) is
established early in Phase 1; renderer integration remains part of the SolUI
spike.
