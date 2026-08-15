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
     │ text-input v4 / input-method v3 protocols
     ▼
sol-ime      (first-party frontend + candidate window, rendered with sol-ui + sol-design)
     │            ↘ engine: reuse fcitx5
     ▼
fcitx5-ime / fcitx5-chinese-addons (pinyin and other mainstream language engines)
```

- `sol-ime` renders the candidate window / preedit with `sol-ui` +
  `sol-design` → the IME's UI is unmistakably SOL.
- The compositor integrates `text-input v4` + `input-method v3` as
  **first-class** protocols in Phase 1 (not Phase 5/6).
- **Mainstream languages first**: Chinese pinyin via `fcitx5-chinese-addons`
  (already on Arch), then Japanese/Korean via existing fcitx5 addons.
- SOL's differentiator is compositor + SDK + first-party visual consistency,
  not an IME research lab.

## Consequences

- `sol-ime` service scaffold lives in `services/sol-ime/` and compiles in the
  workspace (Phase 0 placeholder).
- PRD §7 `sol-core` package list includes `sol-ime`; PRD §40 IME row; §36 MVP
  includes IME; §38 Phase 1 adds text-input v4 + input-method v3.
- PRD §41 #19 tracks the engine/frontend integration boundary (fcitx5 addon
  coverage, engine upgrade strategy, when a custom engine is ever considered).

## Related

- PRD §21.1 (IME), §40 (core technology decisions); ADR-0008 (packaging scope).
