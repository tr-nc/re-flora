# Domain Docs

This is a single-context repository. Engineering skills should read the following domain documentation before exploring or changing the relevant area:

- `CONTEXT.md` at the repository root;
- ADRs under `docs/adr/`, when that directory exists.

If either location is absent, proceed silently. Domain documentation is created lazily when a term or architectural decision actually needs to be recorded.

## Vocabulary

Use the canonical terms defined in `CONTEXT.md` in issue titles, specifications, refactor proposals, hypotheses, and test names. Do not substitute a synonym that the glossary explicitly rejects.

If a needed concept is missing, reconsider whether it is merely an implementation name. Record a genuinely missing domain concept through the domain-modeling workflow.

## ADR conflicts

If proposed work contradicts an existing ADR, identify the conflict explicitly rather than silently overriding it.
