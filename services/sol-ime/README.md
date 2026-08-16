# sol-ime — SOL system input-method bridge

SOL's first-party IME frontend (`sol-ime`) reuses **fcitx5** as the engine
backend; a self-hosted pinyin engine is not supported (pinyin/segmentation/
candidate-ranking is a decade-scale accumulation — fcitx5 is mature).
Decision: PRD §21.1; [ADR-0007](../../docs/decisions/0007-ime-frontend-fcitx5-engine.md).

## Decision: Option A (PRD IME section)

```text
sol-compositor
      │ target: text-input v4 / input-method v3 protocols
      ▼
sol-ime   (first-party frontend + candidate-window/preedit model; sol-ui rendering pending)
      │          ↘ engine: reuse fcitx5
      ▼
fcitx5-ime / fcitx5-chinese-addons（pinyin and other mainstream engines）
```

- The candidate-window / preedit data model uses `sol-design` tokens. Its
  `sol-ui` rendering remains pending.
- The product protocol target is `text-input v4` + `input-method v3`; the
  current Smithay 0.7 compositor implementation advertises `text-input v3` +
  `input-method v2` and will evaluate the newer staging protocols when Smithay
  supports them.
- The engine integration targets the `fcitx5` framework (Chinese pinyin via
  `fcitx5-chinese-addons`, etc.); its transport wiring remains pending.

## Mainstream languages first

v0.1 ships mainstream languages first (Chinese pinyin, etc.), then extends to
Japanese / Korean as needed (fcitx5 supports these already).

## Status

**Phase 1 frontend scaffold.** The candidate-window/preedit data model and
fcitx5 engine seam are present; the client protocol wiring, candidate-window
UI rendering, and fcitx5 transport remain follow-on work (see
[ROADMAP Phase 1 — IME](../../docs/ROADMAP.md)). The integration boundary is
tracked in decision item #19.
