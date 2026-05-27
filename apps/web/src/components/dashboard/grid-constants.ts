// Grid layout primitives shared between dashboard-grid.tsx and sizing-strategies/.
// Persisted (coarse) grid rows are split into SCALE finer rows so content-sized
// widgets quantize to ~ROW_HEIGHT px instead of a whole coarse row.
// Invariant for pixel-identical legacy widgets: legacy per-row step
// (80 + 16) must equal SCALE * (ROW_HEIGHT + MARGIN_Y) → 4 * (8 + 16) = 96.
export const COLS = 12
export const SCALE = 4
export const ROW_HEIGHT = 8
export const MARGIN: readonly [number, number] = [16, 16]
export const MARGIN_X = MARGIN[0]
export const MARGIN_Y = MARGIN[1]
