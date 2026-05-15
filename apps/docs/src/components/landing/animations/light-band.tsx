export function LightBandArrow() {
  return (
    <div aria-hidden className="relative mx-2 hidden h-px flex-1 bg-white/10 md:block">
      <span className="light-band absolute inset-y-0 left-0 w-1/3 bg-gradient-to-r from-transparent via-amber-300 to-transparent" />
    </div>
  )
}
