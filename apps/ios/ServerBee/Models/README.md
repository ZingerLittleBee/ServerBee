# iOS Models — Coding Convention

All `Codable` model types in this directory MUST declare an explicit
`enum CodingKeys: String, CodingKey` covering every Swift property whose
JSON wire-format name differs from the property name (and ideally all
properties, for documentation value).

## Why

Explicit `CodingKeys`:

1. Survive Swift property renames — a refactor of `var memoryUsed` to
   `var memUsed` will not silently break decoding against the backend.
2. Document the exact JSON contract right next to the Swift model.
3. Permit per-field opt-outs and renames (e.g. `cpuUsage = "cpu_usage"`)
   that key-strategy converters cannot express cleanly.

## Encoder / decoder

- `JSONEncoder()` (default, no key-encoding strategy) — properties encode
  exactly as `CodingKeys` declares them.
- `JSONDecoder()` (default) — same.
- Do **not** add `.convertToSnakeCase` / `.convertFromSnakeCase`.

The helpers `JSONEncoder.snakeCase` / `JSONDecoder.snakeCase` are kept for
call-site stability but no longer apply a key strategy — they are plain
default instances. New code should still prefer them so that future
adjustments (e.g. shared date strategies) can be centralised.
