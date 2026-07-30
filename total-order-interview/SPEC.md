# total-order-interview specification

## Purpose

`total-order-interview` is an interactive CLI for eliciting a user's preferences over
binary-feature candidates. It establishes a total preorder over satisfying candidates
where possible, records explicit rejections separately, and produces an observational
report about which variables contribute to the order and in what directly observed
contexts.

The tool does not use LLMs, statistical models, learned weights, classifiers, or
predictions for unseen candidates.

## Core concepts

- A session contains a fixed list of binary features.
- Each feature has a stable generated id and a display name.
- Candidates are the full Cartesian product of feature values.
- Candidate ids are deterministic bit-pattern ids such as `c0101`, using session
  feature order.
- A satisfying candidate may be placed in a total preorder.
- Rejected candidates are excluded from the preorder and reported separately.
- Unknown or insufficiently constrained candidates are listed separately when a
  report is generated before completion.

## Candidate universe

V1 supports only binary yes/no features. The user must provide all features before
the first answer is recorded. Feature display names may be edited only before the
first answer. After answers exist, feature ids remain canonical; future display
aliases may be added without rewriting historical events.

The CLI should warn above 8 features and default to a hard cap of 12 features unless
the user explicitly overrides it.

The all-no candidate has no special baseline semantics.

## User answers

Each sampled prompt displays two complete candidates in a compact table:

```text
        Sandals  Flare pants  Nail polish  At home
A       Y        N            Y            N
B       N        N            Y            N
```

Valid answers:

- `Prefer A`: both candidates are satisfying, and A is ordered above B.
- `Prefer B`: both candidates are satisfying, and B is ordered above A.
- `No preference`: both candidates are satisfying and equal in the preorder.
- `Reject A`: A is unsatisfying and excluded.
- `Reject B`: B is unsatisfying and excluded.
- `Reject both`: both candidates are unsatisfying and excluded.
- `Skip`: records no ordering or rejection evidence.

Rejection is permanent unless retracted through the append-only event log.
`No preference` is evidence of equality, not a skipped comparison.

## Ordering semantics

The preorder is represented as:

- equality classes of satisfying candidates
- a strict partial order between equality classes

Equality is transitive. If `A = B` and `B = C`, then `A = C` is established.
Ordering is transitive. If `A > B` and `B > C`, then `A > C` is established.

The tool assumes user preferences are deterministic enough to form a total preorder,
but it must detect contradictions. It must not continue sampling while the derived
state is inconsistent.

Contradictions include:

- adding an ordering edge that creates a cycle
- merging equality classes that are already strictly ordered
- ordering a candidate relative to a rejected candidate

When a contradiction is detected, the CLI should show the smallest practical
conflicting path and ask the user to keep the new answer or retract an earlier
event from the conflict.

## Sampling strategy

The sampler has two phases.

### Coverage phase

The coverage phase prioritizes one-variable matched pairs because these provide
direct evidence about which variables matter.

For feature `X`, a matched pair is two candidates that differ only in `X`. For `n`
features, each feature has `2^(n-1)` matched-pair contexts.

Selection policy:

- choose the least-tested feature first
- within that feature, choose the least-covered context
- rotate features rather than exhausting all contexts for one feature at once
- avoid immediately repeating skipped pairs
- include only non-rejected candidates

### Ordering phase

After matched-pair coverage is adequate, the sampler prioritizes completing the
total preorder over satisfying candidates. It should use transitive and equality
closure to avoid redundant comparisons. Binary-insertion-style comparisons against
the current ordered tiers are acceptable.

Every candidate included in the ranked preorder must have been directly observed in
at least one prompt. Unobserved candidates remain unplaced.

The interview may stop early. Reports generated early must clearly mark incomplete
coverage and unplaced candidates.

## Attribution rules

The report may attribute importance to variables only from direct matched-pair
evidence.

For a matched pair differing only in variable `X`:

- preference for one side is evidence that `X` affected the preorder in that exact
  context
- `No preference` is direct evidence that `X` did not affect the preorder in that
  exact context
- rejection evidence is not ranking-driver evidence and belongs in a separate
  acceptability section
- skipped pairs provide no evidence

Comparisons where candidates differ by multiple variables help establish order, but
do not directly explain which variable mattered.

Primary-driver statements require direct matched-pair evidence showing frequent
order/tier changes with directional consistency or clear conditional context.

Conditional statements should be generated by grouping matched-pair evidence for a
target variable by each other single variable's value. V1 should not generalize to
higher-order context rules in prose, though the raw matched-pair table exposes the
full contexts.

Example acceptable language:

```text
When At home=no, Sandals changed the ordering in 3 of 4 observed matched pairs.
When At home=yes, Sandals had no observed effect in 4 of 4 observed matched pairs.
```

The report must distinguish:

- observed effect
- observed no effect
- mixed evidence
- not tested

## Reports

V1 emits Markdown and JSON.

The Markdown report is written for a professional therapist or similar reader, but
must use strictly observational language. It must not include diagnostic,
therapeutic, causal, or psychological inferences.

Required report sections:

- session metadata
- feature list
- ranked satisfying candidates as preorder tiers
- rejected candidates
- unplaced candidates
- matched-pair coverage by variable
- variable-driver observations
- exclusion observations
- skipped or inconclusive observations
- method limits
- raw event/comparison appendix

The method limits section must state:

- observations are based only on user responses during this session
- no diagnostic, causal, therapeutic, or psychological conclusions are made
- no preference model, classifier, or prediction for unseen candidates is used
- untested or unplaced candidates are listed separately
- variable-driver claims require direct matched-pair evidence

JSON export contains the full machine-readable audit trail and derived report data.

## Persistence

Sessions are plain JSON files. The persistence format is append-only events plus
derived state recomputed on load.

Core event kinds:

- `session_created`
- `features_defined`
- `comparison_answered`
- `answer_retracted`
- `report_generated`

Every event has a stable event id and timestamp. Retraction refers to event id.
The CLI may expose `undo` as "retract latest answer", but contradiction resolution
must support retracting a specific earlier event.

The CLI writes and flushes the session file after every answered prompt. Writes
should use an atomic pattern: write a temporary file, then rename it into place.

`report_generated` records timestamp, output path, and report format. It does not
affect derived interview state.

## CLI shape

Commands:

```text
total-order-interview new session.json [--seed SEED]
total-order-interview interview session.json
total-order-interview report session.json --md report.md --json report.json
total-order-interview inspect session.json
```

Mutation boundaries:

- `new` creates a session
- `interview` appends answers and retractions
- `report` may append only `report_generated`
- `inspect` is read-only

Interactive `interview` may use `rustyline`. Supported prompt commands should
include:

- `a`
- `b`
- `equal`
- `reject a`
- `reject b`
- `reject both`
- `skip`
- `undo`
- `quit`
- `help`

Candidate A/B side assignment should be randomized to reduce position bias, but
deterministically from session seed and prompt state. `new --seed` stores an
explicit reproducibility seed. If omitted, the seed is generated.

## Implementation dependencies

Prefer minimal dependencies. Approved v1 dependencies:

- `serde`
- `serde_json`
- `chrono`
- `rustyline`

Avoid graph libraries, TUI frameworks, CLI frameworks, or inference libraries unless
the implementation later proves they are necessary.

## Test priorities

Correctness tests should precede CLI polish.

Required coverage:

- equality transitivity: `A = B`, `B = C` implies `A = C`
- order transitivity: `A > B`, `B > C` implies `A > C`
- rejected candidates are excluded from ordering
- adding `C > A` after `A > B > C` detects a cycle
- merging equality classes preserves compatible ordering constraints
- merging equality classes detects existing order conflicts
- matched-pair attribution ignores multi-variable comparisons
- `No preference` in a matched pair counts as observed no-effect evidence
- skipped pairs create no ordering or attribution evidence
- append-only retraction removes the retracted answer from derived state
