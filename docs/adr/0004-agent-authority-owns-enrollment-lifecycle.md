# ADR-0004: Agent Authority owns the enrollment lifecycle

## Status

Accepted (2026-07-13)

## Context

Agent enrollment knowledge was split across a shallow `EnrollmentService`, the
Server and Agent HTTP adapters, `AgentManager`, and best-effort audit writes.
The module exposed storage and hashing primitives while callers owned the hard
parts: pending-versus-enrolled checks, offer replacement, run-token mutation,
WebSocket eviction, transaction order, and audit detail.

That split permitted contradictory behaviour:

- expired offers failed verification but still blocked or appeared as the
  current offer;
- missing optimistic-concurrency input silently enabled last-writer-wins;
- a consumed offer could later also be marked revoked;
- graceful re-enrollment replaced the stored token without immediately fencing
  the previous live connection;
- WebSocket token validation raced final connection admission;
- operator-facing token rotation created a valid credential that no Agent
  necessarily possessed;
- a committed enrollment whose HTTP response was lost left the Server claimed
  by a run token the Agent never received;
- lifecycle audit writes happened after commit and their failures were ignored;
- a machine fingerprint was collected and stored despite no longer identifying,
  deduplicating, authorizing, or otherwise affecting an Agent.

The deletion test confirms the module is shallow: deleting it moves a few
queries and hashes, while the lifecycle complexity remains spread across every
caller.

## Decision

Create a deep **Agent Authority** module. Its external seam is a typed use-case
facade; a private state-machine kernel and persistence implementation sit behind
that interface.

The facade has operation-specific inputs, receipts, and rejection types for the
following use cases:

- claim a Server identity using an Enrollment code and an Agent-generated run
  token;
- issue an offer for an Unclaimed Server with no Outstanding offer;
- begin Graceful or Emergency re-enrollment;
- replace one exact Outstanding offer;
- revoke one exact Outstanding offer;
- revoke Agent authority without creating an offer;
- inspect current Agent authority and offer facts;
- read Agent authority history.

The facade also owns connection authorization through a two-stage admission
interface. Preflight may reject a bad token before WebSocket upgrade, but it is
not authorization. The returned pending admission must perform final token
revalidation while holding the same per-Server lock used by authority
transitions, then register the connection with `AgentManager`. Callers cannot
register an Agent connection without crossing that final admission seam.

Public methods never accept a database transaction, token hash, enrollment
entity, or `AgentManager`. Axum adapters translate transport DTOs into typed
inputs and translate typed results back into HTTP responses; they do not edit
authority rows or orchestrate side effects.

### State model

Agent authority and Enrollment offer state are orthogonal:

- Agent authority is **Claimed** when a valid Agent run token exists and
  **Unclaimed** when it does not. Online/offline remains an `AgentManager`
  connection fact.
- An Enrollment offer begins **Outstanding** and reaches exactly one immutable
  outcome: **Consumed**, **Revoked**, **Replaced**, or **Expired**.
- At most one offer may be Outstanding for a Server.
- Replacement requires the exact current offer identity. Missing or stale
  identity conflicts; there is no last-writer-wins variant.
- Expiry takes effect at `expires_at` without a background task. The next write
  may materialize the Expired outcome before proceeding.

Graceful re-enrollment leaves existing authority intact while its offer is
Outstanding. Consuming the offer replaces the run token and fences the previous
connection. Emergency re-enrollment removes authority and fences the connection
when the offer is issued. Agent authority revocation removes authority and
fences the connection without creating an offer.

Operator-facing run-token rotation is removed. Authority can be restored only
through enrollment.

### Credential ownership

The Agent generates and durably stages a high-entropy run token before claiming
an offer. The claim request carries that token, and Agent Authority atomically
stores its hash while consuming the offer. The Server never returns the
plaintext run token.

After an ambiguous network result, the Agent tries its already-staged token on
the WebSocket. Success proves that the claim committed; rejection permits a
retry with the same still-usable Enrollment code and proposed token. A process
restart follows the same path because the proposed token was staged before the
request.

This is a hard protocol cut. A claim without an Agent-generated run token is
rejected; there is no legacy adapter that generates and returns one. Existing
Agents with valid run tokens remain connected, but any future enrollment
requires an Agent implementing the new claim protocol.

Agent fingerprint generation, transport, validation, persistence, tests, and
current documentation are removed. Historical specifications and changelog
entries remain historical records.

### Transactions, history, and fencing

Every durable transition and its secret-free **Agent authority event** commit
in one SQLite transaction. An event records the actor, request source, Server
snapshot, related offer, mode, outcome, and time. Generic best-effort audit logs
are not the lifecycle source of truth.

Credential hashes and Enrollment offer rows are deleted with their Server.
Agent authority events survive Server deletion and retain no usable secret;
they are removed only by an explicit audit-retention purge.

Connection fencing is a hard invariant, not a post-commit courtesy. An
authority transition and final WebSocket admission serialize through the same
per-Server lock. A transition returns success only after its durable mutation,
authority event, and runtime fence complete. Failure is fail-closed: a valid
Agent may be briefly disconnected if a later database operation fails, but its
unchanged token permits reconnection.

### Server onboarding

**Server onboarding** remains a separate deep module because it owns Server
identity creation, profile and tag persistence, default monitoring
configuration, and the initial Enrollment offer. Its external interface is
request-idempotent: the same request identity and normalized input return the
same Server; reusing that identity with different input conflicts.

Server onboarding composes initial-offer creation through a crate-private
transaction seam so the Server and offer remain atomic. No database transaction
appears in either module's external interface.

SQLite is a local-substitutable dependency and is exercised through the real
test database. `AgentManager` is an in-process dependency and is exercised with
real channels. Neither dependency receives a hypothetical port or mock adapter.

## Alternatives considered

- **One generic `transition(AuthorityIntent)` entry point**: minimizes method
  count but creates broad command and receipt enums. Callers must understand
  irrelevant variants, weakening locality at the external seam.
- **A sealed generic command protocol**: preserves typed receipts while adding
  new commands without adding methods, but associated types, sealed traits, and
  generic error wrappers cost more discoverability and AI navigability than the
  lifecycle's expected rate of change justifies.
- **A mutable aggregate handle (`load`, mutate, `commit`)**: naturally groups
  state but leaks lock and transaction lifetime, ordering, and commit
  responsibility to callers. It recreates the existing orchestration problem.
- **Keep route orchestration and only expand `EnrollmentService`**: leaves
  connection fencing, run-token transitions, and authority history outside the
  module, so the deletion test still fails.
- **Retain a legacy server-generated-token claim adapter**: eases rollout but
  preserves the ambiguous-response failure and requires two credential
  contracts. The enrollment protocol intentionally makes a clean cut instead.
- **Consume the machine fingerprint for identity, token binding, or anomaly
  detection**: the value is self-reported, spoofable, and has inconsistent
  platform semantics. No current product behaviour justifies retaining it.

## Consequences

- HTTP and WebSocket adapters become thin and tests move to the Agent Authority
  interface. Primitive `EnrollmentService` tests and duplicated route
  orchestration tests are deleted once equivalent interface coverage exists.
- The Server schema needs explicit offer outcomes, immutable authority events,
  request-idempotent onboarding storage, and removal of the fingerprint column.
  Migrations remain forward-only.
- The Agent registration request and local persistence flow change in concert
  with the Server. Install and re-enrollment flows must use the new Agent binary.
- Web and iOS must send exact offer identities for replacement and use Agent
  re-enrollment terminology. `recover` is not a domain term; if an old route
  spelling is temporarily retained, it is only an HTTP adapter to the same
  facade.
- Final WebSocket admission performs an additional token validation. This cost
  is accepted to close the authorization race.
- The module's direct SQLite and `AgentManager` dependencies favor the real
  architecture over speculative substitutability. If a second runtime or
  remote implementation appears, that concrete need can introduce a new seam.
