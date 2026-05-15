export function LightBandArrow() {
  return (
    <div aria-hidden className="relative mx-2 hidden h-px flex-1 self-center bg-white/10 md:block">
      <span className="light-band absolute -top-px h-[3px] w-1/3 rounded-full bg-gradient-to-r from-transparent via-amber-300 to-transparent shadow-[0_0_12px_2px_rgba(255,179,0,0.4)]" />
    </div>
  )
}
