# 7. IME: first-party sol-ime frontend + fcitx5 engine

- **Status:** Accepted
- **Date:** 2026-08-15
- **Target phase:** Phase 1

## Context

Input method is a system level that shapes "daily-driver desktop" quality; it
cannot be deferred to Phase 5/6. PRD §21.1 therefore treats IME as a
first-class citizen of the compositor / SolKit / sol-ui / sol-design stack,
not a retrofit. SOL rejects both "skip IME" (it is part of the consistent
first-party app experience, unlike X11 legacy) and "self-host a pinyin
engine" (pinyin segmentation/candidate-ranking is a decade-scale accumulation;
libpinyin / rime / fcitx5 already excel at it).

## Decision

**Option A — first-party IME frontend (`sol-ime`) reusing `fcitx5` as the
engine backend, no self-hosted engine.**

```text
sol-compositor
     │ target: text-input v4 / input-method v3 protocols
     ▼
sol-ime      (first-party frontend + candidate-window/preedit model; sol-ui rendering pending)
     │            ↘ engine: reuse fcitx5
     ▼
fcitx5-ime / fcitx5-chinese-addons (pinyin and other mainstream language engines)
```

- `sol-ime` owns the candidate-window/preedit frontend model. Rendering it
  with `sol-ui` + `sol-design` is follow-on work.
- The product protocol target is `text-input v4` + `input-method v3`. The
  current Smithay 0.7 implementation advertises and dispatches `text-input
  v3` + `input-method v2`; SOL evaluates the newer staging protocols when
  Smithay supports them. This remains a **first-class** Phase 1 concern (not
  Phase 5/6).
- **Mainstream languages first**: Chinese pinyin via `fcitx5-chinese-addons`
  (already on Arch), then Japanese/Korean via existing fcitx5 addons.
- SOL's differentiator is compositor + SDK + first-party visual consistency,
  not an IME research lab.

## Consequences

- `sol-ime` provides the Phase 1 candidate-window/preedit data model and a
  session-bus fcitx5 transport. Candidate-window rendering and full Wayland
  input-method client-surface delivery remain follow-on work.
- PRD §7 `sol-core` package list includes `sol-ime`; PRD §40 IME row; §36 MVP
  includes IME; §38 Phase 1 adds the current Smithay `text-input v3` +
  `input-method v2` integration while keeping v4/v3 as the protocol target.
- PRD §41 #19 tracks the engine/frontend integration boundary (fcitx5 addon
  coverage, engine upgrade strategy, when a custom engine is ever considered).

## Related

- PRD §21.1 (IME), §40 (core technology decisions); ADR-0008 (packaging scope).
