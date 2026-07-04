// Strips ANSI escape sequences (CSI/SGR color codes, cursor moves, OSC, etc.).
// Container log streams preserve whatever bytes the process wrote to stdout/stderr,
// so logs from any process that emits color escapes would otherwise render raw
// litter like `\x1b[2m` / `[32m INFO`. This is a display-only concern; the raw
// bytes are left untouched everywhere else. Pattern adapted from `ansi-regex`.
//
// The ESC/CSI/BEL control bytes are produced with `String.fromCharCode` and
// interpolated into the pattern so no literal control character appears in
// source (and the builder stays a `RegExp`, not a lint-flagged regex literal).
const ESC = String.fromCharCode(0x1b)
const CSI = String.fromCharCode(0x9b)
const BEL = String.fromCharCode(0x07)

const ANSI_PATTERN = new RegExp(
  `[${ESC}${CSI}][[\\]()#;?]*(?:(?:[a-zA-Z\\d]*(?:;[-a-zA-Z\\d/#&.:=?%@~_]*)*)?${BEL}|(?:\\d{1,4}(?:;\\d{0,4})*)?[\\dA-PR-TZcf-ntqry=><~])`,
  'g'
)

export function stripAnsi(input: string): string {
  return input.replace(ANSI_PATTERN, '')
}
