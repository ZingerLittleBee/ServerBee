'use client'

import { memo } from 'react'

export interface DateTickerProps {
  currentIndex: number
  labels: string[]
  visible: boolean
}

const DateTickerLabel = memo(function DateTickerLabel({ currentIndex, labels }: Omit<DateTickerProps, 'visible'>) {
  const label = labels[currentIndex] ?? labels[0] ?? ''

  return (
    <div className="overflow-hidden rounded-full bg-zinc-900 px-4 py-1 text-white shadow-lg dark:bg-zinc-100 dark:text-zinc-900">
      <div className="flex h-6 items-center justify-center">
        <span className="whitespace-nowrap font-medium text-sm">{label}</span>
      </div>
    </div>
  )
})

export function DateTicker({ currentIndex, labels, visible }: DateTickerProps) {
  if (!visible || labels.length === 0) {
    return null
  }

  return <DateTickerLabel currentIndex={currentIndex} labels={labels} />
}

DateTicker.displayName = 'DateTicker'

export default DateTicker
