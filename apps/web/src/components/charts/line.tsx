'use client'

import { curveNatural } from '@visx/curve'
import { LinePath } from '@visx/shape'
import type { CurveFactory } from 'd3-shape'
import { type RefObject, useCallback, useId, useMemo, useRef, useState } from 'react'
import { chartCssVars, useChartStable, useYScale } from './chart-context'
import type { LoadingStyle } from './chart-phase'
import { type FadeEdges, fadeGradientStops, resolveFadeSides, viewportFadeGradientAttrs } from './fade-edges'
import { type LineLoadingPulseMode, LineLoadingPulseStroke, resolveLineLoadingPulseMode } from './line-loading-pulse'
import { LINE_LOADING_LOOP_PAUSE_MS } from './line-loading-timing'
import { LineLoadingSweep } from './loading-sweep'
import { resolveDashTailBounds, usePathStrokeMetrics } from './path-stroke-utils'
import { SeriesDashTailOverlay } from './series-dash-tail-overlay'
import { SeriesHighlightLayer } from './series-highlight-layer'
import { SeriesHoverDim } from './series-hover-dim'
import { SeriesMarkers } from './series-markers'
import type { SeriesPointMarkerStyle } from './series-point-marker'
import { useAnimatedSeriesPath } from './use-animated-series-path'

export function isLineDatumDefined(datum: Record<string, unknown>, dataKey: string): boolean {
  const value = datum[dataKey]
  return typeof value === 'number' && Number.isFinite(value)
}

export interface LineProps {
  /** Whether to animate the line. Default: true */
  animate?: boolean
  /** Connect finite points across missing values. Default: false */
  connectNulls?: boolean
  /** Curve function. Default: curveNatural */
  curve?: CurveFactory
  /** Dash pattern for the tail segment when `dashFromIndex` is set. Default: "6,4" */
  dashArray?: string
  /**
   * Data index from which the line stroke becomes dashed (inclusive).
   * Useful for projecting incomplete periods, e.g. dashed from yesterday through today.
   */
  dashFromIndex?: number
  /** Key in data to use for y values */
  dataKey: string
  /**
   * Fade the line stroke toward transparent at the chart edges.
   * - `true` fades both edges, `false` disables the fade entirely.
   * - `"left"` / `"right"` fades only that side.
   * Default: true
   */
  fadeEdges?: FadeEdges
  /**
   * Show the loading pulse overlay. Default: follows chart loading phase.
   * Set `false` to disable even during loading.
   */
  loading?: boolean
  /** Override pulse animation mode (loop / exit / enter). */
  loadingPulseMode?: LineLoadingPulseMode
  /** Stroke color for the loading pulse overlay. Default: var(--foreground) */
  loadingStroke?: string
  /** Loading pulse stroke opacity. Default: 0.5 */
  loadingStrokeOpacity?: number
  /**
   * Loading animation while the chart is in loading status: the default
   * traveling `"pulse"`, or a diagonal `"sweep"` shimmer across the skeleton
   * line. Default: `"pulse"`.
   */
  loadingStyle?: LoadingStyle
  /** Marker styling (same options as Scatter). */
  markers?: SeriesPointMarkerStyle
  /** Called when a loop-mode pulse cycle completes. */
  onLoadingPulseCycleComplete?: () => void
  /** Whether to show highlight segment on hover. Default: true */
  showHighlight?: boolean
  /** Render scatter-style circle markers at each data point. Default: false */
  showMarkers?: boolean
  /** Stroke color. Default: var(--chart-line-primary) */
  stroke?: string
  /** Stroke width. Default: 2.5 */
  strokeWidth?: number
  /** Y-scale group id (Recharts `yAxisId`). Default: `"left"`. */
  yAxisId?: string | number
}

function LineSeriesStroke({
  animatedPathD,
  curve,
  isDefined,
  getY,
  pathRef,
  renderData,
  strokeWidth,
  useDataTransitionPath,
  visibleStroke,
  xAccessor,
  xScale
}: {
  animatedPathD: string
  curve: CurveFactory
  isDefined: (datum: Record<string, unknown>) => boolean
  getY: (datum: Record<string, unknown>) => number
  pathRef: RefObject<SVGPathElement | null>
  renderData: Record<string, unknown>[]
  strokeWidth: number
  useDataTransitionPath: boolean
  visibleStroke: string
  xAccessor: (datum: Record<string, unknown>) => Date
  xScale: (value: Date) => number | undefined
}) {
  if (useDataTransitionPath && animatedPathD) {
    return (
      <path
        d={animatedPathD}
        fill="none"
        ref={pathRef}
        stroke={visibleStroke}
        strokeLinecap="round"
        strokeWidth={strokeWidth}
      />
    )
  }

  return (
    <LinePath
      curve={curve}
      data={renderData}
      defined={isDefined}
      innerRef={pathRef}
      stroke={visibleStroke}
      strokeLinecap="round"
      strokeWidth={strokeWidth}
      x={(d) => xScale(xAccessor(d)) ?? 0}
      y={getY}
    />
  )
}

function LineLoadingOverlays({
  curve,
  handleLoadingPulseComplete,
  innerWidth,
  loadingStroke,
  loadingStrokeOpacity,
  loadingStyle,
  pathD,
  pulseEpoch,
  pulseMode,
  showLoadingPulse,
  strokeWidth
}: {
  curve: CurveFactory
  handleLoadingPulseComplete: () => void
  innerWidth: number
  loadingStroke: string
  loadingStrokeOpacity: number
  loadingStyle: LoadingStyle
  pathD: string | null
  pulseEpoch: number
  pulseMode: LineLoadingPulseMode | null
  showLoadingPulse: boolean
  strokeWidth: number
}) {
  const sweepLoading = showLoadingPulse && innerWidth > 0 && loadingStyle === 'sweep'
  const pulseLoading = showLoadingPulse && innerWidth > 0 && !sweepLoading

  return (
    <>
      {sweepLoading ? (
        <LineLoadingSweep
          curve={curve}
          key="loading-sweep"
          mode={pulseMode ?? 'loop'}
          onTransitionComplete={handleLoadingPulseComplete}
          stroke={loadingStroke}
          strokeOpacity={loadingStrokeOpacity}
          strokeWidth={strokeWidth}
        />
      ) : null}
      {pulseLoading && pathD ? (
        <LineLoadingPulseStroke
          key="loading-pulse"
          loopEpoch={pulseEpoch}
          mode={pulseMode ?? undefined}
          onCycleComplete={handleLoadingPulseComplete}
          pathD={pathD}
          stroke={loadingStroke}
          strokeOpacity={loadingStrokeOpacity}
          strokeWidth={strokeWidth}
        />
      ) : null}
    </>
  )
}

export function Line({
  dataKey,
  yAxisId,
  stroke = chartCssVars.linePrimary,
  strokeWidth = 2.5,
  curve = curveNatural,
  animate = true,
  connectNulls = false,
  fadeEdges = true,
  showHighlight = true,
  showMarkers = false,
  markers,
  dashFromIndex,
  dashArray = '6,4',
  loading,
  loadingStroke = chartCssVars.foreground,
  loadingStrokeOpacity = 0.5,
  loadingPulseMode,
  onLoadingPulseCycleComplete,
  loadingStyle = 'pulse'
}: LineProps) {
  // Stable slice only: hover state lives inside `<SeriesHoverDim>` and
  // `<SeriesHighlightLayer>` so this component (and its expensive
  // <SeriesDashTailOverlay> child) does not re-render on cursor motion.
  // The reveal-clip is now a single shared clipPath at the chart-shell
  // level (`time-series-chart-shell.tsx`); we no longer render a per-line
  // `<ChartRevealClip>` or read `revealEpoch` here.
  const {
    data,
    renderData,
    xScale,
    innerHeight,
    innerWidth,
    xAccessor,
    lines,
    chartPhase,
    notifyLoadingPulseComplete,
    yDomainTweenDuration
  } = useChartStable()
  const yScale = useYScale(yAxisId)
  const isDefined = useCallback((datum: Record<string, unknown>) => isLineDatumDefined(datum, dataKey), [dataKey])
  const seriesRenderData = useMemo(
    () => (connectNulls ? renderData.filter(isDefined) : renderData),
    [connectNulls, isDefined, renderData]
  )
  const useDataTransitionPath = animate && chartPhase === 'ready' && seriesRenderData.every(isDefined)
  const { pathD: animatedPathD } = useAnimatedSeriesPath({
    chartPhase,
    curve,
    dataKey,
    durationMs: yDomainTweenDuration,
    enabled: useDataTransitionPath,
    innerWidth,
    renderData: seriesRenderData,
    xAccessor,
    xScale,
    yScale
  })

  const phasePulseMode = resolveLineLoadingPulseMode(chartPhase)
  const pulseMode = loading === false ? null : (loadingPulseMode ?? (loading === true ? 'loop' : phasePulseMode))
  const showLoadingPulse = pulseMode != null
  const [pulseEpoch, setPulseEpoch] = useState(0)
  const effectiveShowHighlight = showHighlight && !showLoadingPulse

  const handleLoadingPulseComplete = useCallback(() => {
    onLoadingPulseCycleComplete?.()
    if (pulseMode === 'loop') {
      window.setTimeout(() => {
        setPulseEpoch((epoch) => epoch + 1)
      }, LINE_LOADING_LOOP_PAUSE_MS)
      return
    }
    notifyLoadingPulseComplete?.()
  }, [notifyLoadingPulseComplete, onLoadingPulseCycleComplete, pulseMode])

  const seriesIndex = useMemo(() => {
    const index = lines.findIndex((line) => line.dataKey === dataKey)
    return index >= 0 ? index : 0
  }, [lines, dataKey])

  const pathRef = useRef<SVGPathElement>(null)
  const { pathLength, pathD } = usePathStrokeMetrics(pathRef)

  const reactId = useId()
  const gradientId = `line-gradient-${dataKey}-${reactId}`

  const getY = useCallback(
    (d: Record<string, unknown>) => {
      const value = d[dataKey]
      return typeof value === 'number' ? (yScale(value) ?? 0) : 0
    },
    [dataKey, yScale]
  )

  const hasDashTail = resolveDashTailBounds(dashFromIndex, data.length)
  const fadeSides = resolveFadeSides(fadeEdges)
  const lineStroke = fadeSides.any ? `url(#${gradientId})` : stroke
  const fadeStops = fadeSides.any ? fadeGradientStops(fadeSides) : null
  const showSeriesStroke = chartPhase === 'revealing' || chartPhase === 'ready' || chartPhase === 'exitingReady'
  let visibleStroke = 'transparent'
  if (showSeriesStroke && !hasDashTail) {
    visibleStroke = lineStroke
  }

  return (
    <>
      {fadeStops ? (
        <defs>
          <linearGradient id={gradientId} {...viewportFadeGradientAttrs(innerWidth)}>
            {fadeStops.map((stop) => (
              <stop key={stop.offset} offset={stop.offset} style={{ stopColor: stroke, stopOpacity: stop.opacity }} />
            ))}
          </linearGradient>
        </defs>
      ) : null}

      <SeriesHoverDim dimOpacity={0.3} enabled={effectiveShowHighlight} seriesIndex={seriesIndex}>
        <LineSeriesStroke
          animatedPathD={animatedPathD}
          curve={curve}
          getY={getY}
          isDefined={isDefined}
          pathRef={pathRef}
          renderData={seriesRenderData}
          strokeWidth={strokeWidth}
          useDataTransitionPath={useDataTransitionPath}
          visibleStroke={visibleStroke}
          xAccessor={xAccessor}
          xScale={xScale}
        />

        <SeriesDashTailOverlay
          dashArray={dashArray}
          dashFromIndex={dashFromIndex}
          data={data}
          innerHeight={innerHeight}
          innerWidth={innerWidth}
          pathD={pathD}
          pathLength={pathLength}
          stroke={lineStroke}
          strokeWidth={strokeWidth}
          xAccessor={xAccessor}
          xScale={xScale}
        />
      </SeriesHoverDim>

      {showMarkers ? (
        <SeriesMarkers
          animate={animate}
          dataKey={dataKey}
          {...markers}
          fill={markers?.fill ?? stroke}
          stroke={markers?.stroke ?? markers?.fill ?? stroke}
        />
      ) : null}

      <SeriesHighlightLayer
        enabled={effectiveShowHighlight}
        height={innerHeight}
        pathRef={pathRef}
        stroke={stroke}
        strokeWidth={strokeWidth}
      />

      <LineLoadingOverlays
        curve={curve}
        handleLoadingPulseComplete={handleLoadingPulseComplete}
        innerWidth={innerWidth}
        loadingStroke={loadingStroke}
        loadingStrokeOpacity={loadingStrokeOpacity}
        loadingStyle={loadingStyle}
        pathD={pathD}
        pulseEpoch={pulseEpoch}
        pulseMode={pulseMode}
        showLoadingPulse={showLoadingPulse}
        strokeWidth={strokeWidth}
      />
    </>
  )
}

Line.displayName = 'Line'

export default Line
