# sol-ime — SOL system input-method bridge

SOL's first-party IME frontend (`sol-ime`) reuses **fcitx5** as the engine
backend; a self-hosted pinyin engine is not supported (pinyin/segmentation/
candidate-ranking is a decade-scale accumulation — fcitx5 is mature).
Decision: PRD §21.1; [ADR-0007](../../docs/decisions/0007-ime-frontend-fcitx5-engine.md).

## Decision: Option A (PRD IME section)

```text
sol-compositor
      │ target: native SCP text-input capability
      ▼
sol-ime   (first-party frontend + candidate-window/preedit model; sol-ui rendering pending)
      │          ↘ engine: reuse fcitx5
      ▼
fcitx5-ime / fcitx5-chinese-addons（pinyin and other mainstream engines）
```

- The candidate-window / preedit data model uses `sol-design` tokens. Its
  `sol-ui` rendering remains pending.
- Text input is an SCP capability with explicit focus and commit authority.
  Its wire messages and compositor routing remain follow-on work; `sol-ime`
  does not open a legacy compositor-protocol connection in the meantime.
- The engine integration uses fcitx5's public session-bus
  `org.fcitx.Fcitx.InputMethod1` / `InputContext1` contract. It forwards key
  presses and translates `UpdateFormattedPreedit`, `UpdateClientSideUI`, and
  `CommitString` signals into the frontend model.

## Mainstream languages first

v0.1 ships mainstream languages first (Chinese pinyin, etc.), then extends to
Japanese / Korean as needed (fcitx5 supports these already).

## Status

**Phase 1 transport implementation slice.** `Fcitx5DbusTransport` is the real
session-bus adapter, while `Fcitx5Transport` lets unit tests use a
deterministic fake.
The fake covers Chinese pinyin `shan → 山/闪/善 → 山`; an ignored smoke test
can be run against a live fcitx5 session. Candidate-window UI rendering and
the full SCP text-input client surface remain follow-on work (see
[ROADMAP Phase 1 — IME](../../docs/ROADMAP.md)). The integration boundary is
tracked in decision item #19.
