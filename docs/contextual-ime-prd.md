# SOL Contextual IME — Product Requirements Document

**Version:** v0.1
**Status:** Proposal
**Product:** SOL Operating System
**Component:** `sol-ime`
**Initial language:** Simplified Chinese Pinyin
**Processing model:** Local-first, offline-capable, fail-open
**Related decision:** [ADR-0007 — first-party frontend + fcitx5 engine](decisions/0007-ime-frontend-fcitx5-engine.md)

---

## 1. Executive summary

SOL Contextual IME is a context-aware candidate-ranking layer for the SOL
desktop input method. It keeps fcitx5 and its mature language engines as the
authority for composition, segmentation, conversion, and candidate validity.
SOL adds local context, personalization, and eventually a small purpose-built
semantic model to decide which valid candidate is most likely in the current
writing situation.

The product is not a generative assistant embedded in a keyboard. It does not
write sentences on the user's behalf, contact a cloud model for each keypress,
or replace the language engine. Its first product claim is intentionally
narrow:

> Reduce candidate-selection effort without making normal typing slower, less
> predictable, or less private.

The first release validates this claim by reranking the current fcitx5
candidate page in shadow mode and then in an opt-in experiment. Larger
candidate pools, a distilled semantic encoder, document retrieval, and
long-term semantic memory are gated follow-up work rather than MVP
prerequisites.

---

## 2. Problem statement

Traditional IMEs are strong at mapping keystrokes to linguistically valid
candidates, but the same Pinyin sequence often remains ambiguous without the
user's immediate context.

For example:

```text
Previous text: 这个新的缓存方案
Pinyin:        xiao lv
Candidates:    效率 / 效力 / 小率 / 校率
Likely intent: 效率
```

The user pays for unresolved ambiguity through:

- selecting a non-first candidate;
- paging through candidates;
- committing and immediately correcting text;
- repeatedly teaching different applications the same names and terminology;
- losing topic context when switching between phrases within one document.

fcitx5 should continue to decide what conversions are possible. SOL should use
the context already available at the trusted input-method boundary to improve
their ordering.

---

## 3. Product definition

### 3.1 Positioning

SOL Contextual IME is:

> A local, context-aware ranking and personalization layer over mature input
> method engines.

The long-term architecture may use embeddings, but the product is defined by
its user outcome rather than by a specific model architecture.

### 3.2 Product boundary

```text
Keystrokes
    │
    ├──────────────► fcitx5 language engine
    │                 composition / segmentation / valid candidates
    │
    └──────────────► SOL context engine
                      nearby text / app context / local preferences
                              │
fcitx5 candidates ─────────────┤
                              ▼
                       SOL candidate ranker
                              │
                              ▼
                       SOL candidate window
```

fcitx5 remains the constraint engine. SOL owns presentation, contextual
ranking, privacy policy, local personalization, measurement, and graceful
fallback.

---

## 4. Goals and non-goals

### 4.1 Goals

1. Improve first-candidate accuracy for ambiguous Simplified Chinese Pinyin.
2. Reduce explicit candidate selections, candidate paging, and immediate
   correction after commit.
3. Preserve an immediate traditional candidate path even when contextual
   ranking is unavailable, late, or wrong.
4. Keep inference and personal data local by default.
5. Provide clear controls to disable contextual ranking, disable it per app,
   and erase learned data.
6. Establish a reusable ranking boundary that can later support Japanese,
   Korean, names, technical terms, and document-aware input.
7. Gather privacy-bounded, opt-in evidence before committing to a custom model
   or deep fcitx5 integration.

### 4.2 Non-goals

The following are not part of the MVP:

- replacing fcitx5, libpinyin, Rime, Anthy, KKC, or Hangul engines;
- generating phrases that are absent from the engine candidate set;
- cloud inference on the interactive typing path;
- sentence completion, rewriting, translation, or chatbot behavior;
- silently reading the clipboard, accessibility tree, screen, or arbitrary
  application documents;
- a custom 32K-token Transformer KV cache;
- FlashAttention, paged-attention kernels, or GPU inference;
- cross-device personalization sync;
- Japanese or Korean launch support;
- collecting raw user text for centralized training by default.

---

## 5. Product principles

### 5.1 Traditional-first response

The user must receive the normal fcitx5 result immediately. Contextual ranking
is a bounded refinement, not a dependency for typing.

### 5.2 Stable interaction

Candidates must not move after the user starts navigating, presses a numeric
selection key, clicks the candidate window, or after the refinement deadline
has passed.

### 5.3 Fail open

If the ranker crashes, times out, has no model, rejects the context, or returns
invalid data, the original fcitx5 order is used without dropping input.

### 5.4 Minimum necessary context

SOL uses the smallest context source that can support the feature. Generic
applications provide bounded Wayland surrounding text. SOL applications may
explicitly provide document chunks through a future SolKit API. Neither path
authorizes unrelated system-wide collection.

### 5.5 Measured intelligence

A more complex model ships only when replay and opt-in field data show a
meaningful improvement over the previous, simpler ranker under the same
latency and memory budget.

### 5.6 Reversible personalization

Learning is local, inspectable at the settings level, pausable, and erasable.
Disabling personalization must not disable basic input.

---

## 6. Target users and jobs

### 6.1 Primary users

- Simplified Chinese Pinyin users who write across chat, documents, code, and
  technical tools.
- Users with recurring names, project vocabulary, abbreviations, and domain
  terminology.
- Privacy-sensitive users who want improved input without cloud processing.

### 6.2 Jobs to be done

When I type an ambiguous Pinyin sequence, I want the candidate that fits what I
am currently writing to appear first so I can keep typing without stopping to
correct the IME.

When I repeatedly select a name or technical term, I want the input method to
adapt locally without exposing the surrounding text to a service.

When I enter credentials or sensitive data, I want contextual processing and
learning to stop automatically.

When contextual intelligence is uncertain or unavailable, I want the input
method to behave exactly like the reliable traditional engine beneath it.

---

## 7. User experience

### 7.1 Normal composition

1. The user types Pinyin.
2. fcitx5 produces preedit and candidate state.
3. SOL displays the original first candidate immediately.
4. If a contextual result arrives before the refinement deadline and the user
   has not interacted with the list, SOL may replace the ordering.
5. Selection commits the corresponding original fcitx5 candidate, regardless
   of its displayed position.

Contextual ranking must not add an “AI loading” indicator to normal typing.

### 7.2 User begins selection before refinement

As soon as the user moves the candidate cursor, pages, clicks, or presses a
selection key, the visible order freezes for that composition revision. A late
ranker result is discarded.

### 7.3 Ranker unavailable

The candidate list remains in fcitx5 order. No error toast is shown during
normal typing. Repeated failures may surface as a non-blocking diagnostic in
Input Method settings.

### 7.4 Sensitive field

When the text-input content purpose is password, PIN, or another sensitive
purpose, or the sensitive-data hint is present:

- no surrounding text is sent to the ranker;
- no composition or selection event is persisted;
- no personal-memory lookup occurs;
- any in-flight contextual request is cancelled or invalidated;
- the candidate path falls back to the underlying engine policy.

### 7.5 Settings

Input Method settings must eventually provide:

- Context-aware ranking: On / Off;
- Personal learning: On / Off;
- per-application disable list;
- clear learned vocabulary and ranking history;
- storage usage;
- model version and status;
- a concise local-processing and privacy explanation.

The MVP may expose these through configuration before the complete settings UI
exists, but all controls must have typed service boundaries.

---

## 8. Scope and release stages

| Capability | Foundation | MVP | Follow-up |
|---|---:|---:|---:|
| Real Wayland input-method round trip | Required | Required | Maintain |
| SOL candidate-window rendering | Required | Required | Polish |
| Simplified Chinese Pinyin | Required | Required | Maintain |
| Stable candidate source identity | Required | Required | Maintain |
| Current-page Top-9 reranking | — | Required | Maintain |
| Shadow evaluation | — | Required | Maintain |
| Local frequency and recency | — | Required | Improve |
| Semantic encoder | — | Optional experiment | Required only after gate |
| Full fcitx5 candidate-pool access | — | — | Fcitx addon experiment |
| Document chunk retrieval | — | — | SolKit applications first |
| Long-term semantic memory | — | — | Opt-in experiment |
| Japanese and Korean | — | — | After Chinese quality gate |
| Cloud model or sync | — | — | Separate product decision |

---

## 9. Functional requirements

### 9.1 End-to-end input foundation

**IME-FND-001** — `sol-ime` must run as a supervised desktop-session process
and connect to the compositor's supported input-method protocol.

**IME-FND-002** — The input path must support focus, key events, preedit,
candidate updates, commit, reset, cursor rectangle, surrounding text, content
type, and popup lifecycle.

**IME-FND-003** — A missing or failed contextual component must not make a text
field unusable.

**IME-FND-004** — The production path must be validated in at least one GTK 4,
one Qt 6, and one Electron text field before contextual ranking is enabled by
default.

### 9.2 Candidate identity and revisions

**IME-CAN-001** — Every displayed candidate must retain its source-engine
index and original rank.

**IME-CAN-002** — Every composition update must have a monotonic revision. A
ranking result may be applied only to the revision that produced it.

**IME-CAN-003** — Reranking must be a permutation of the supplied candidates.
The MVP must not insert, remove, rewrite, or synthesize candidates.

**IME-CAN-004** — Selecting a reranked candidate must commit the same source
candidate that would have been selected at its original engine index.

**IME-CAN-005** — Duplicate display strings must remain distinct candidate
identities.

### 9.3 Context

**IME-CTX-001** — Generic applications may contribute only bounded Wayland
surrounding text, content type, cursor position, change cause, and an
OS-authenticated application identity.

**IME-CTX-002** — The context engine may maintain an in-memory ring of text
committed during the active, non-sensitive session.

**IME-CTX-003** — Context from one application must not automatically flow
into another application's ranking request. Cross-app personal vocabulary is
a separate, user-controlled signal and must not contain raw session text.

**IME-CTX-004** — Focus loss, reset, privacy-policy changes, and sensitive
content must invalidate incompatible in-flight requests.

**IME-CTX-005** — A future SolKit document provider must require explicit
application participation and expose bounded chunks rather than unrestricted
document handles.

**IME-CTX-006** — If the compositor cannot authenticate the focused
application identity, `sol-ime` must not create a contextual ranking request,
read or update personalization, or reuse session context. It must use the
traditional engine order until identity is available. An empty, guessed, or
client-supplied display name is not a substitute identity.

### 9.4 Ranking

**IME-RNK-001** — The ranker receives an immutable context snapshot,
composition state, and candidate batch, and returns candidate source IDs with
scores and a confidence value.

**IME-RNK-002** — The first production experiment must combine original-rank
prior, local context features, personal frequency, and recency. It must not
require a neural model.

**IME-RNK-003** — Semantic weight must be reduced when context is absent,
model confidence is low, or the predicted margin is below the configured
threshold.

**IME-RNK-004** — The ranker must return deterministic output for identical
input, model, memory snapshot, and configuration.

**IME-RNK-005** — Invalid, duplicated, missing, stale, or late source IDs must
cause the entire refinement to be rejected.

**IME-RNK-006** — The original order must remain available for debugging,
offline evaluation, and immediate fallback.

### 9.5 Personalization

**IME-PER-001** — Personal learning must be local by default and disabled in
sensitive contexts.

**IME-PER-002** — A selection event may update bounded statistics for the
selected text, composition, application class, and recency only when the
privacy policy permits it.

**IME-PER-003** — Raw surrounding text must not be placed in long-term
personalization storage in the MVP.

**IME-PER-004** — Users must be able to pause learning and erase all learned
IME data without erasing the fcitx5 language engine configuration.

**IME-PER-005** — Personal data must use private per-user storage, atomic
replacement or transactional updates, schema versioning, and bounded
retention.

### 9.6 Shadow mode and experimentation

**IME-EXP-001** — Shadow mode computes a proposed order without changing the
visible candidate list.

**IME-EXP-002** — Evaluation must distinguish four disjoint cohorts:

1. **evaluation-eligible:** the ground-truth selected candidate is present in
   the supplied candidate batch;
2. **baseline-correct:** an eligible composition where the original engine
   Top-1 equals the ground truth;
3. **ranking opportunity:** an eligible composition where the original engine
   Top-1 differs from the ground truth; and
4. **out-of-pool:** the ground truth is absent from the supplied batch and
   therefore cannot be recovered by a permutation-only ranker.

Top-1 accuracy and mean reciprocal rank use only evaluation-eligible records.
Opportunity recovery is the absolute percentage of ranking-opportunity records
whose ground truth the proposed ranker moves to Top-1. Out-of-pool records are
reported separately and never counted as ranker successes or failures.

**IME-EXP-003** — Field experiments must be opt-in until privacy review and
the launch gates in this document are satisfied.

**IME-EXP-004** — Experiment assignment, model version, and ranker version
must be locally attributable without persisting raw surrounding text.

**IME-EXP-005** — The feature must support an immediate kill switch that
returns all users to original engine ordering.

---

## 10. Proposed software contracts

The precise Rust API is an engineering decision, but the product requires
equivalent typed boundaries.

```rust
// Opaque value issued by the compositor/security boundary for the currently
// focused client. It cannot be constructed from a client display name/string.
struct AuthenticatedAppId(SystemIdentityHandle);

struct CandidateId {
    revision: u64,
    source_index: usize,
}

struct Candidate {
    id: CandidateId,
    text: String,
    label: Option<String>,
    original_rank: usize,
}

struct ContextSnapshot {
    revision: u64,
    app_id: AuthenticatedAppId,
    text_before_cursor: String,
    text_after_cursor: String,
    content_class: ContentClass,
    session_terms: Vec<String>,
    personalization_allowed: bool,
}

struct RankingResult {
    revision: u64,
    ordered_ids: Vec<CandidateId>,
    confidence: f32,
    elapsed_micros: u64,
}
```

The UI must never infer source identity from candidate text.

---

## 11. Runtime architecture

### 11.1 MVP path

```text
Wayland client
    │ text-input state
    ▼
SOL compositor
    │ input-method protocol
    ▼
sol-ime
    ├── privacy policy
    ├── context snapshot
    ├── Fcitx bridge
    ├── candidate revision store
    └── ranker worker
          │
          ├── heuristic/context features
          └── local personal statistics
```

The key/event loop, Fcitx signal handling, and ranking work must be
asynchronous. A fixed quiet-window wait on the key handling path is not an
acceptable production synchronization mechanism.

### 11.2 Follow-up full-pool path

The current D-Bus client-side UI contract is suitable for validating visible
candidate reranking but does not guarantee a large engine candidate pool. If
Top-9 results pass the launch gate, SOL may prototype a narrowly scoped fcitx5
addon that reads bulk candidates and applies a returned permutation.

```text
fcitx5 engine
    ▼
fcitx5 SOL candidate adapter
    │ bounded local IPC
    ▼
SOL context/ranking service
```

The adapter must preserve candidate actions and source identity, impose a
strict timeout, and remain fail-open. A custom language engine remains out of
scope.

### 11.3 Follow-up semantic memory

Long documents are searchable memory, not an attention window. The first
document-aware design should use hashed text chunks, cached embeddings, and
Top-K retrieval into a small active context.

```text
Document chunks ──► local index
                         │
Current input ───────────┤ retrieve Top-K
                         ▼
                  bounded active context
                         ▼
                    context encoder
```

Persistent 32K Transformer KV, paged attention, and custom attention kernels
require a separate benchmark-backed architecture decision.

---

## 12. Semantic model requirements

This section applies only after the non-neural MVP passes its product gate.

### 12.1 Initial model budget

| Property | Initial target |
|---|---:|
| Parameters | 3M–8M |
| Layers | 2 |
| Hidden width | approximately 192 |
| Context embedding | 128 dimensions |
| Quantization | INT8 |
| Interactive device | CPU |
| Network requirement | None |

The model should encode context and composition once. Candidate embeddings may
be precomputed or encoded through a bounded candidate tower. The output head
scores supplied candidates; it does not generate text.

### 12.2 Training objective

Training must optimize the actual candidate decision:

```text
context + composition + candidate set -> selected candidate
```

Preferred objectives include listwise cross-entropy over the candidate set,
hard-negative contrastive loss, and optional teacher-distribution
distillation. Generic sentence-similarity performance is not a launch metric.

### 12.3 Training data

Permitted sources must be documented and reviewable. Candidate records should
contain the minimum fields needed to reproduce ranking:

```text
bounded context representation
composition
candidate set and original ranks
selected source candidate
non-sensitive behavior features
```

Centralized collection of raw user surrounding text is not authorized by this
PRD. Any future opt-in contribution program requires a separate consent,
redaction, retention, security, and deletion design.

### 12.4 Model lifecycle

- Models are versioned independently from the service binary.
- A checksum and compatibility manifest are required before activation.
- Activation is atomic and rollback-safe.
- An incompatible or corrupt model disables semantic refinement rather than
  blocking input.
- Evaluation reports must identify dataset version, model version, ranker
  version, and baseline.

---

## 13. Performance and resource requirements

### 13.1 Interactive latency

| Measurement | Target | Hard guardrail |
|---|---:|---:|
| Key dispatch overhead added by SOL | P95 < 1 ms | P99 < 2 ms |
| First traditional candidate | P95 < 4 ms | P99 < 8 ms |
| Top-9 contextual rerank | P95 < 4 ms | P99 < 8 ms |
| Refinement application deadline | 8 ms | Never after user interaction |
| Commit after selection | P95 < 4 ms | P99 < 8 ms |

Measurements start after the compositor has delivered the relevant input
event and exclude application rendering time. Product reports must also show
end-to-end desktop-session latency so component-local measurements do not hide
transport delays.

### 13.2 Resource budget

| Resource | MVP target | Semantic follow-up guardrail |
|---|---:|---:|
| Ranker resident memory | < 20 MB | < 80 MB |
| Personal store | < 20 MB default | < 100 MB configurable |
| Idle CPU | approximately 0% | < 0.5% average |
| Per-key allocation | bounded | no growth with session length |
| Network traffic | 0 | 0 on interactive path |

### 13.3 Degradation policy

The ranker must skip refinement when the system is under configured resource
pressure, the request misses its deadline, context is invalid, or model state
is unavailable. Skipping is a normal control path, not an error.

---

## 14. Privacy and security

### 14.1 Data classes

| Data | In memory | Persistent MVP | Network |
|---|---:|---:|---:|
| Current composition | Yes | No raw log | No |
| Wayland surrounding text | Bounded | No | No |
| Session committed text | Bounded | No | No |
| Candidate set | Yes | Optional bounded evaluation record | No |
| Selected term statistics | Yes | Yes, when enabled | No |
| Sensitive-field content | No ranking use | Never | Never |
| Document chunks | Future opt-in | Future local cache | No by default |

### 14.2 Required safeguards

- Default-deny sensitive-field handling.
- Private per-user file permissions.
- Bounded retention and schema versioning.
- No opaque diagnostic payload containing user text.
- No shell API that exposes raw IME context.
- Clear-data operation covers personal statistics, evaluation records, model
  caches derived from personal documents, and semantic indexes.
- Tests must use synthetic text and include explicit secret-leak fixtures.
- Crash reporting must record typed error codes, not composition or context.

### 14.3 Trust boundary

The input method is privileged because applications intentionally send it
surrounding text. That access must not be generalized into a desktop-wide text
collection service. Other SOL components may request ranked results only
through a future, separately authorized typed API; they may not read the IME's
raw context store.

---

## 15. Success metrics

### 15.1 Primary metrics

- First-candidate selection rate.
- Mean reciprocal rank of the selected candidate.
- Candidate actions per committed Chinese character.
- Candidate page changes per 1,000 compositions.
- Immediate correction rate: backspace or replacement shortly after commit.

### 15.2 Guardrail metrics

- Traditional first-candidate latency.
- Rerank latency and deadline-miss rate.
- Stale-result rejection count.
- Candidate source-index mismatch count.
- Ranker crash and fallback rate.
- Memory and idle CPU.
- Sensitive-context persistence count.
- Contextual-ranking disable rate.

### 15.3 Segmentation

Metrics must be reported separately for:

- all compositions and evaluation-eligible compositions;
- baseline-correct and ranking-opportunity compositions;
- out-of-pool compositions;
- short versus long context;
- applications or application classes;
- cold versus learned users;
- model/ranker versions;
- original Top-1 correct versus original Top-1 incorrect cases.

An aggregate improvement must not hide a regression where already-correct
fcitx5 candidates are displaced.

---

## 16. Launch gates

### Gate A — foundation readiness

- End-to-end Chinese Pinyin works in real GTK 4 and Qt 6 applications.
- Candidate rendering, selection, paging, reset, focus, and commit are stable.
- There is no fixed 40 ms synchronization wait on the interactive key path.
- Source candidate identity survives all frontend transformations.
- All existing and new deterministic tests pass.

### Gate B — shadow-mode value

- At least 10,000 evaluation-eligible compositions from consented replay
  records or documented synthetic data are evaluated.
- The heuristic ranker recovers at least 5 absolute percentage points of
  ranking-opportunity compositions without reducing evaluation-eligible
  all-case Top-1 accuracy or the baseline-correct preservation rate.
- Source-index mismatch count is zero.
- Sensitive-context persistence count is zero.
- Rerank P95 is below 4 ms on the reference device.

### Gate C — opt-in visible experiment

- Candidate-order freezing is validated under keyboard, pointer, paging, and
  delayed-result races.
- Immediate correction does not regress by more than 1% relative.
- Traditional first-candidate P95 regresses by no more than 1 ms.
- Users can disable ranking and erase learned data.
- The original-order kill switch is tested.

### Gate D — semantic model

- The model beats the heuristic baseline, not only the original fcitx5 order.
- Opportunity recovery improves by at least another 3 absolute percentage
  points over the heuristic baseline.
- All latency, memory, privacy, and stability guardrails remain satisfied.
- Model packaging, validation, rollback, and no-model fallback are verified.

### Gate E — default enablement

- A multi-week field trial shows sustained benefit across supported
  applications.
- Disable rate and qualitative feedback show no material predictability issue.
- Security and privacy review is complete.
- ADR-0007's engine/frontend boundary has been amended or supplemented with an
  accepted contextual-ranking decision.

---

## 17. Delivery plan

### Milestone 0 — production IME foundation

- Add the supervised `sol-ime` executable.
- Complete compositor/input-method client wiring.
- Replace request/quiet-window synchronization with revisioned asynchronous
  events.
- Complete SOL candidate-window rendering and positioning.
- Add real desktop-session smoke tests.

### Milestone 1 — ranking contract

- Introduce stable candidate IDs and composition revisions.
- Add context snapshot and privacy-policy types.
- Add a pluggable ranker interface and original-order implementation.
- Add race, timeout, duplicate-text, and source-index tests.
- Add latency tracing and a deterministic replay harness.

### Milestone 2 — heuristic shadow mode

- Add original-rank, local-context, frequency, and recency features.
- Add bounded local evaluation records.
- Run shadow evaluation and publish a gate report.
- Do not change visible ordering until Gate B passes.

### Milestone 3 — opt-in contextual ranking

- Enable visible Top-9 refinement for opted-in users.
- Add settings, clear-data, per-app disable, and kill switch.
- Validate field latency, stability, correction rate, and user trust.

### Milestone 4 — semantic student model

- Build the training and replay dataset pipeline.
- Train and export the first 3M–8M INT8 student.
- Add local inference, model manifests, rollback, and fallback.
- Ship only after Gate D passes.

### Milestone 5 — full-pool and document experiments

- Prototype a fail-open fcitx5 candidate addon.
- Prototype explicit SolKit document-context providers.
- Add hashed chunk cache and local semantic retrieval.
- Evaluate Japanese and Korean only after Chinese quality is stable.

---

## 18. Test strategy

### 18.1 Deterministic tests

- Pinyin composition to preedit, candidates, and commit.
- Reranked display index to original source index.
- Duplicate candidate display text.
- Stale revision and late ranker result rejection.
- User interaction freezing candidate order.
- Ranker timeout, crash, corrupt model, and unavailable model.
- Focus and application switching.
- Sensitive content and clear-data behavior.
- Store schema upgrade, bounds, permissions, and atomicity.

### 18.2 Replay evaluation

The replay harness must run original order and every proposed ranker over the
same versioned candidate records. Reports include accuracy, MRR, regressions,
latency distributions, memory, and segmentation from Section 15.

### 18.3 Desktop-session validation

At minimum:

- GTK 4 multiline text editor;
- Qt 6 text field and multiline editor;
- Electron text field;
- password and PIN fields;
- rapid typing, backspace, cursor movement, focus switching, and paging;
- ranker process termination during composition.

---

## 19. Dependencies

- Stable compositor text-input v3 and input-method v2 behavior.
- A runnable, supervised `sol-ime` client.
- fcitx5 and Chinese addon availability in packaging.
- Candidate-window rendering through SolUI/SOL design tokens.
- Validated application identity for per-app policy.
- Settings storage and private state storage.
- Privacy-bounded diagnostics that prohibit raw input payloads.
- Reference hardware and repeatable latency measurement.

---

## 20. Risks and mitigations

| Risk | Impact | Mitigation |
|---|---|---|
| Contextual ranker displaces an already-correct candidate | Loss of trust | Conservative gating, original-rank prior, shadow mode, fast kill switch |
| Candidate moves while the user selects it | Wrong commit | Revision IDs, interaction freeze, strict refinement deadline |
| Display index no longer matches engine index | Wrong text committed | Stable source IDs and permutation validation |
| D-Bus/event batching adds visible latency | Input feels broken | Asynchronous signal path; traditional candidate never waits for ranking |
| Sensitive text reaches persistent storage | Severe privacy failure | Default-deny content policy, no raw context persistence, leak tests |
| Small model does not outperform heuristics | Wasted complexity | Gate model work on replay evidence; retain heuristic baseline |
| D-Bus exposes only current candidate page | Limits ranking gain | Validate Top-9 first; prototype fcitx5 addon only after product gate |
| Personalization becomes stale or overfits | Poor predictions | Bounded recency, decay, per-app segmentation, reset controls |
| Long-context infrastructure consumes excessive memory | Desktop regression | Chunk retrieval first; explicit budgets; defer persistent KV |
| New scope conflicts with ADR-0007 | Architectural drift | Preserve fcitx5 authority and require a follow-up ADR before deep integration |

---

## 21. Open product decisions

1. Is visible contextual reranking opt-in or opt-out after Gate C?
2. What is the reference low-end device for the 4 ms P95 target?
3. What maximum Wayland surrounding-text length should the MVP accept?
4. Should personal vocabulary be global, per app, or a weighted combination?
5. How should users inspect or edit learned terms without exposing raw history?
6. Which applications form the initial field-test cohort?
7. Is a fcitx5 addon supportable within SOL's packaging and upgrade policy?
8. What synthetic and licensed corpora may seed the first semantic model?
9. Does any future contribution program justify handling raw text, or should
   all training signals remain local or transformed?
10. What minimum field-trial duration is required before default enablement?

These decisions do not block the foundation or shadow-mode milestones.

---

## 22. MVP acceptance criteria

The Contextual IME MVP is complete when all of the following are true:

- SOL provides a stable real-session Simplified Chinese Pinyin input path over
  fcitx5 in supported GTK 4 and Qt 6 applications.
- The first traditional candidate remains immediate and independent of the
  contextual ranker.
- Candidate revisions and source identities prevent stale updates and wrong
  commits under deterministic race tests.
- A local heuristic ranker runs in shadow mode over the current candidate page.
- Replay reporting covers product metrics and latency guardrails.
- Sensitive fields bypass contextual processing and produce no persistent
  evaluation or personalization record.
- Ranker timeout, absence, or failure produces the unmodified fcitx5 order.
- Users or testers can disable the feature and erase its local learned data.
- Gate B has a written result, even if the result is to stop further semantic
  model investment.

The MVP does not require a Transformer. Its deliverable is a trustworthy,
measurable contextual-ranking product boundary and evidence about whether that
boundary creates enough user value to justify the next stage.
