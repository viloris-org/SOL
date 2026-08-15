# sol-ime — SOL system input-method bridge

SOL's first-party IME frontend (`sol-ime`) reuses **fcitx5** as the engine
backend; a self-hosted pinyin engine is not supported (pinyin/segmentation/
candidate-ranking is a decade-scale accumulation — fcitx5 is mature).
Decision: PRD §21.1; [ADR-0007](../../docs/decisions/0007-ime-frontend-fcitx5-engine.md).

## Decision: Option A (PRD IME section)

```text
sol-compositor
      │ text-input v4 / input-method v3 protocols
      ▼
sol-ime   (first-party frontend + candidate window, rendered with sol-ui + sol-design)
      │          ↘ engine: reuse fcitx5
      ▼
fcitx5-ime / fcitx5-chinese-addons（pinyin and other mainstream engines）
```

- The candidate window / preedit is rendered with `sol-ui` + `sol-design` —
  visual consistency with first-party apps.
- The compositor participates as a first-class citizen of `text-input v4` +
  `input-method v3`.
- Engine support comes from the `fcitx5` framework (Chinese pinyin via
  `fcitx5-chinese-addons`, etc.).

## Mainstream languages first

v0.1 ships mainstream languages first (Chinese pinyin, etc.), then extends to
Japanese / Korean as needed (fcitx5 supports these already).

## Status

**Phase 0 scaffold** (compiles; not implemented). Protocol wiring and the
candidate-window UI land in Phase 1 (see [ROADMAP Phase 1 — IME](../../docs/ROADMAP.md)). The
integration boundary is tracked in decision item #19.
