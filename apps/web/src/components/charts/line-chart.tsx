'use client'

import { ParentSize } from '@visx/responsive'
import type { Transition } from 'motion/react'
import {
  Children,
  type CSSProperties,
  isValidElement,
  type ReactElement,
  type ReactNode,
  useCallback,
  useMemo,
  useRef,
  useState
} from 'react'
import { cn } from '@/lib/utils'
import type { LineConfig, Margin } from './chart-context'
import { ChartLoadingLabel } from './chart-loading-label'
import {
  type ChartPhase,
  type ChartStatus,
  DEFAULT_CHART_STATUS,
  DEFAULT_Y_DOMAIN_TWEEN_MS,
  resolveRestingChartPhase
} from './chart-phase'
import { Line, type LineProps } from './line'
import { TimeSeriesChartInner } from './time-series-chart-shell'
import type { YDomain } from './y-domain-utils'

export interface LineChartProps {
  /** Animation duration in milliseconds. Default: 1100 */
  animationDuration?: number
  /** CSS easing for clip-reveal. Default: cubic-bezier(0.85, 0, 0.15, 1) */
  animationEasing?: string
  /** Aspect ratio as "width / height". Default: "2 / 1". Omit to fill a sized parent. */
  aspectRatio?: string
  /** Child components (Line, Grid, ChartTooltip, etc.) */
  children: ReactNode
  /** Additional class name for the container */
  className?: string
  /** Data array - each item should have a date field and numeric values */
  data: Record<string, unknown>[]
  enterTransition?: Transition
  /** Centered shimmer label while loading. */
  loadingLabel?: string
  /** Chart margins */
  margin?: Partial<Margin>
  /** Fires when the internal chart phase changes (e.g. OG capture readiness). */
  onPhaseChange?: (phase: ChartPhase) => void
  revealSignature?: string
  /** Loading vs ready — drives chart phase and loading chrome. Default: `"ready"`. */
  status?: ChartStatus
  /** Inline container styles (e.g. fixed height for brush strip). */
  style?: CSSProperties
  /** Tween y-domain when brush changes the visible x-range. Default: false */
  tweenYDomainOnXDomainChange?: boolean
  /** Key in data for the x-axis (date). Default: "date" */
  xDataKey?: string
  /** Visible x-domain for brush zoom. */
  xDomain?: [Date, Date]
  /** Full dataset length for x-scale padding when `xDomain` is set. */
  xDomainSlotCount?: number
  /** Fixed domain for the primary y-axis. */
  yDomain?: YDomain
  /** Animate y-domain when status or target domain changes. Default: true */
  yDomainTween?: boolean
  /** Animate y-domain over this duration (ms) on status transitions. Default: 500. */
  yDomainTweenDuration?: number
}

const DEFAULT_MARGIN: Margin = { top: 40, right: 40, bottom: 40, left: 40 }

/** Series renderers that carry a dataKey but must not drive the shared y-domain. */
const LINE_DOMAIN_EXCLUDED_NAMES = new Set([
  'ProfitLossLine',
  'LineSeriesTerminalMarker',
  'Area',
  'SeriesBar',
  'Scatter',
  'Candlestick',
  'Bar',
  'PatternArea'
])

function getChildComponentName(child: ReactElement) {
  const childType = child.type as { displayName?: string; name?: string }
  return typeof child.type === 'function' ? childType.displayName || childType.name || '' : ''
}

function registersLineDomain(child: ReactElement, props: LineProps | undefined) {
  if (!props?.dataKey) {
    return false
  }

  const componentName = getChildComponentName(child)
  if (componentName === 'Line' || child.type === Line) {
    return true
  }
  if (LINE_DOMAIN_EXCLUDED_NAMES.has(componentName)) {
    return false
  }
  // MDX / duplicate bundle instances may not share the same `Line` reference.
  return typeof props.dataKey === 'string' && props.dataKey.length > 0
}

function extractLineConfigs(children: ReactNode): LineConfig[] {
  const configs: LineConfig[] = []

  const visit = (node: ReactNode) => {
    Children.forEach(node, (child) => {
      if (!isValidElement(child)) {
        return
      }

      const props = child.props as LineProps | undefined

      if (registersLineDomain(child, props) && props?.dataKey) {
        configs.push({
          dataKey: props.dataKey,
          stroke: props.stroke || 'var(--chart-line-primary)',
          strokeWidth: props.strokeWidth || 2.5,
          yAxisId: props.yAxisId
        })
        return
      }

      const childProps = child.props as { children?: ReactNode } | undefined
      if (childProps?.children) {
        visit(childProps.children)
      }
    })
  }

  visit(children)
  return configs
}

interface ChartInnerProps {
  animationDuration: number
  animationEasing?: string
  chartStatus: ChartStatus
  children: ReactNode
  containerRef: React.RefObject<HTMLDivElement | null>
  data: Record<string, unknown>[]
  enterTransition?: Transition
  height: number
  loadingLabel?: string
  margin: Margin
  onPhaseChange: (phase: ChartPhase) => void
  revealSignature?: string
  tweenYDomainOnXDomainChange?: boolean
  width: number
  xDataKey: string
  xDomain?: [Date, Date]
  xDomainSlotCount?: number
  yDomain?: YDomain
  yDomainTween: boolean
  yDomainTweenDuration: number
}

function ChartInner({
  width,
  height,
  data,
  xDataKey,
  margin,
  animationDuration,
  animationEasing,
  enterTransition,
  revealSignature,
  chartStatus,
  loadingLabel,
  yDomainTweenDuration,
  yDomainTween,
  yDomain,
  xDomain,
  xDomainSlotCount,
  tweenYDomainOnXDomainChange,
  children,
  containerRef,
  onPhaseChange
}: ChartInnerProps) {
  const lines = useMemo(() => extractLineConfigs(children), [children])

  return (
    <TimeSeriesChartInner
      animationDuration={animationDuration}
      animationEasing={animationEasing}
      chartStatus={chartStatus}
      clipPathId="chart-grow-clip"
      containerRef={containerRef}
      data={data}
      enterTransition={enterTransition}
      height={height}
      lines={lines}
      loadingLabel={loadingLabel}
      margin={margin}
      onPhaseChange={onPhaseChange}
      revealSignature={revealSignature}
      tweenYDomainOnXDomainChange={tweenYDomainOnXDomainChange}
      width={width}
      xDataKey={xDataKey}
      xDomain={xDomain}
      xDomainSlotCount={xDomainSlotCount}
      yDomainTween={yDomainTween}
      yDomainTweenDuration={yDomainTweenDuration}
      yScaleDomain={yDomain}
    >
      {children}
    </TimeSeriesChartInner>
  )
}

export function LineChart({
  data,
  xDataKey = 'date',
  margin: marginProp,
  animationDuration = 1100,
  animationEasing,
  enterTransition,
  revealSignature,
  aspectRatio = '2 / 1',
  className = '',
  status = DEFAULT_CHART_STATUS,
  loadingLabel,
  yDomainTweenDuration = DEFAULT_Y_DOMAIN_TWEEN_MS,
  yDomainTween = true,
  yDomain,
  xDomain,
  xDomainSlotCount,
  tweenYDomainOnXDomainChange = false,
  style,
  onPhaseChange,
  children
}: LineChartProps) {
  const containerRef = useRef<HTMLDivElement>(null)
  const margin = { ...DEFAULT_MARGIN, ...marginProp }
  const [chartPhase, setChartPhase] = useState<ChartPhase>(() => resolveRestingChartPhase(status))
  const handlePhaseChange = useCallback(
    (phase: ChartPhase) => {
      setChartPhase(phase)
      onPhaseChange?.(phase)
    },
    [onPhaseChange]
  )

  const showLoadingLabel = Boolean(
    loadingLabel?.trim() &&
      (chartPhase === 'loading' ||
        chartPhase === 'exiting' ||
        chartPhase === 'gridTweenReady' ||
        chartPhase === 'revealingLoading')
  )

  return (
    <div
      className={cn('relative w-full', className)}
      ref={containerRef}
      style={{
        ...(aspectRatio ? { aspectRatio } : undefined),
        touchAction: 'none',
        ...style
      }}
    >
      <ParentSize debounceTime={10}>
        {({ width, height }) => (
          <ChartInner
            animationDuration={animationDuration}
            animationEasing={animationEasing}
            chartStatus={status}
            containerRef={containerRef}
            data={data}
            enterTransition={enterTransition}
            height={height}
            loadingLabel={loadingLabel}
            margin={margin}
            onPhaseChange={handlePhaseChange}
            revealSignature={revealSignature}
            tweenYDomainOnXDomainChange={tweenYDomainOnXDomainChange}
            width={width}
            xDataKey={xDataKey}
            xDomain={xDomain}
            xDomainSlotCount={xDomainSlotCount}
            yDomain={yDomain}
            yDomainTween={yDomainTween}
            yDomainTweenDuration={yDomainTweenDuration}
          >
            {children}
          </ChartInner>
        )}
      </ParentSize>
      {showLoadingLabel ? <ChartLoadingLabel exiting={chartPhase !== 'loading'} text={loadingLabel} /> : null}
    </div>
  )
}

export default LineChart
