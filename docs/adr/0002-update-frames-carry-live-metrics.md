# Browser update frames carry LiveMetrics, not a zero-filled ServerStatus

`BrowserMessage::Update` used to carry the full `ServerStatus` with every static
field (mem_total, os, tags, has_token, ...) hard-coded to `0`/`None`/`true`,
because a `SystemReport` genuinely does not contain them — totals and identity
travel once via `SystemInfo` and live in the DB. The "Update is partial" rule
was enforced nowhere in the types, so every client re-encoded it defensively:
the web merge hand-picked 20 live fields, and iOS guarded `memoryTotal`/`diskTotal`
with `> 0` checks but missed `swapTotal`, which every update frame stomped to 0.

Decision: update frames carry a dedicated `LiveMetrics` type (Rust
`crates/common/src/types.rs`, mirrored in TS `server-catalog.ts`) containing only
the fields an agent report can populate. `FullSync` and REST keep the full
`ServerStatus`. The wire stays compatible by subtraction — same `update` tag,
same `servers` key, same field names, just fewer keys — so older iOS builds
decode the missing statics as `nil` and their merge keeps cached values (which
also fixes the swapTotal stomp without an app release).

Subtraction has one hard floor: shipped iOS decoders require `id` **and**
`name` (`ServerStatus.init(from:)` uses non-optional `decode` for both) and
drop the whole frame on a missing key. `LiveMetrics` therefore keeps `name` as
a documented decoder-compat field — it is the agent connection name, not live
data, and clients must not let it overwrite the REST-managed name on merge
(the web merge pins `current.name`; an iOS decoding regression test pins the
contract).

Consequences: an update can no longer seed the web catalog cache; frames
arriving before `full_sync`/REST are dropped (previously they seeded rows with
zero placeholders — the same bug class client-side).
Considered and rejected: (a) making the static fields `Option` on `ServerStatus`
— one type stays two-faced and every FullSync consumer pays an unwrap; (b)
server-side named constructors only — concentrates the projection but keeps the
zero placeholders on the wire and all the defensive client code alive.
