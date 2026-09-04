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
Motion::Panel
Motion::Material
Motion::Morph
Motion::Rebound
Motion::Window
Motion::Workspace
Motion::SessionHandoff
```

Apps must not name a millisecond duration; they name an **animation
semantic role**, and duration + curve resolve from `sol-design` tokens.

## Status

**Phase 2 semantic-motion foundation implemented.** Motion specifications,
tiers, animation-driver contracts, and interruptible-animation state are
present. `SpringValue` provides deterministic sub-stepped spring integration,
live-value retargeting, gesture takeover, and velocity handoff for component
morph/rebound effects. The compositor animation system (gestures / workspace
motion) is established early in Phase 1; renderer integration remains part of
the SolUI spike.
