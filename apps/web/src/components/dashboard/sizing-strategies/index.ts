// Returns the tier closest to `value`. Ties pick the first (smaller) tier —
// conservative bias avoids accidental upgrades when the user releases mid-drag.
export function nearestTier(value: number, tiers: readonly number[]): number {
  return tiers.reduce((best, t) => (Math.abs(t - value) < Math.abs(best - value) ? t : best), tiers[0])
}
