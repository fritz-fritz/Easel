# ADR 0002: Provider compliance is enforced by the registry

- Status: accepted
- Date: 2026-07-14

## Context

Image licenses do not automatically grant API access for a wallpaper product. Several popular
free-photo services explicitly restrict wallpaper applications even though individual images may
be free to use.

## Decision

Every compiled provider has a policy disposition. Only providers marked `Allowed` may be enabled.
`RequiresWrittenApproval`, `Prohibited`, and `Unknown` integrations remain unavailable at runtime.

Remote asset records preserve creator, source, license, attribution, canonical work URL, and any
provider-required use-reporting action.

## Consequences

- Openverse is the initial discovery candidate.
- Unsplash remains disabled until written authorization is obtained for Easel.
- Pexels and Pixabay are not implemented under currently published wallpaper/standalone rules.
- Extremely large stills should prefer open-access / government / museum APIs (NASA, Wikimedia
  Commons, Smithsonian, Met, LOC, Europeana) over stock-photo platforms.
- Terms review becomes part of release readiness.
