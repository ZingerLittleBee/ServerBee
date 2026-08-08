'use client'

import type { ProvidedZoom, TransformMatrix } from '@visx/zoom'
import type { Feature, FeatureCollection, Geometry } from 'geojson'
import type { Transition } from 'motion/react'
import { createContext, type Dispatch, type RefObject, type SetStateAction, useContext, useMemo, useState } from 'react'

// ZoomState from visx/zoom that includes isDragging
interface ZoomState {
  initialTransformMatrix: TransformMatrix
  isDragging: boolean
  transformMatrix: TransformMatrix
}

// Combined type from visx Zoom children prop
export type ZoomInstance<E extends Element> = ProvidedZoom<E> & ZoomState

// Zoom context to share zoom controls with child components
interface ChoroplethZoomContextValue {
  zoom: ZoomInstance<SVGSVGElement> | null
}

export const ChoroplethZoomContext = createContext<ChoroplethZoomContextValue>({
  zoom: null
})

export function useChoroplethZoom() {
  return useContext(ChoroplethZoomContext)
}

export interface Margin {
  bottom: number
  left: number
  right: number
  top: number
}

export interface ChoroplethFeatureProperties {
  id?: string | number
  name?: string
  [key: string]: unknown
}

export type ChoroplethFeature = Feature<Geometry, ChoroplethFeatureProperties>

export interface ChoroplethTooltipData {
  feature: ChoroplethFeature
  featureIndex: number
  x: number
  y: number
}

export interface ChoroplethInteractionContextValue {
  hoveredFeatureIndex: number | null
  setHoveredFeatureIndex: (index: number | null) => void
  setTooltipData: Dispatch<SetStateAction<ChoroplethTooltipData | null>>
  tooltipData: ChoroplethTooltipData | null
}

export interface ChoroplethStableContextValue {
  animationDuration: number

  containerRef: RefObject<HTMLDivElement | null>
  enterTransition?: Transition
  featureCollection: FeatureCollection<Geometry, ChoroplethFeatureProperties>

  /** Precomputed SVG path strings — one per feature index. */
  featurePaths: readonly (string | null)[]
  // Geo data
  features: ChoroplethFeature[]
  height: number
  innerHeight: number
  innerWidth: number

  // Animation
  isLoaded: boolean
  margin: Margin

  // Projection function (returns path string)
  pathGenerator: (feature: ChoroplethFeature) => string | undefined

  // Project geo coordinates to screen coordinates
  projectPoint: (coords: [number, number]) => [number, number] | null

  // Raw path function for graticule (accepts any geo object)
  // biome-ignore lint/suspicious/noExplicitAny: GeoJSON types are complex
  rawPathGenerator: (geo: any) => string | null
  revealEpoch: number

  // Dimensions
  width: number
}

export type ChoroplethContextValue = ChoroplethStableContextValue & ChoroplethInteractionContextValue

const ChoroplethStableContext = createContext<ChoroplethStableContextValue | null>(null)
const ChoroplethInteractionContext = createContext<ChoroplethInteractionContextValue | null>(null)

export function ChoroplethStableProvider({
  children,
  value
}: {
  children: React.ReactNode
  value: ChoroplethStableContextValue
}) {
  return <ChoroplethStableContext.Provider value={value}>{children}</ChoroplethStableContext.Provider>
}

export function ChoroplethInteractionShell({ children }: { children: React.ReactNode }) {
  const [hoveredFeatureIndex, setHoveredFeatureIndex] = useState<number | null>(null)
  const [tooltipData, setTooltipData] = useState<ChoroplethTooltipData | null>(null)

  const interaction = useMemo<ChoroplethInteractionContextValue>(
    () => ({
      hoveredFeatureIndex,
      setHoveredFeatureIndex,
      tooltipData,
      setTooltipData
    }),
    [hoveredFeatureIndex, tooltipData]
  )

  return <ChoroplethInteractionContext.Provider value={interaction}>{children}</ChoroplethInteractionContext.Provider>
}

export function ChoroplethProvider({ children, value }: { children: React.ReactNode; value: ChoroplethContextValue }) {
  const stable = useMemo<ChoroplethStableContextValue>(
    () => ({
      features: value.features,
      featureCollection: value.featureCollection,
      featurePaths: value.featurePaths,
      pathGenerator: value.pathGenerator,
      rawPathGenerator: value.rawPathGenerator,
      projectPoint: value.projectPoint,
      width: value.width,
      height: value.height,
      innerWidth: value.innerWidth,
      innerHeight: value.innerHeight,
      margin: value.margin,
      containerRef: value.containerRef,
      isLoaded: value.isLoaded,
      animationDuration: value.animationDuration,
      enterTransition: value.enterTransition,
      revealEpoch: value.revealEpoch
    }),
    [
      value.features,
      value.featureCollection,
      value.featurePaths,
      value.pathGenerator,
      value.rawPathGenerator,
      value.projectPoint,
      value.width,
      value.height,
      value.innerWidth,
      value.innerHeight,
      value.margin,
      value.containerRef,
      value.isLoaded,
      value.animationDuration,
      value.enterTransition,
      value.revealEpoch
    ]
  )

  const interaction = useMemo<ChoroplethInteractionContextValue>(
    () => ({
      hoveredFeatureIndex: value.hoveredFeatureIndex,
      setHoveredFeatureIndex: value.setHoveredFeatureIndex,
      tooltipData: value.tooltipData,
      setTooltipData: value.setTooltipData
    }),
    [value.hoveredFeatureIndex, value.setHoveredFeatureIndex, value.tooltipData, value.setTooltipData]
  )

  return (
    <ChoroplethStableProvider value={stable}>
      <ChoroplethInteractionContext.Provider value={interaction}>{children}</ChoroplethInteractionContext.Provider>
    </ChoroplethStableProvider>
  )
}

export function useChoroplethStable(): ChoroplethStableContextValue {
  const context = useContext(ChoroplethStableContext)
  if (!context) {
    throw new Error('useChoroplethStable must be used within a ChoroplethProvider')
  }
  return context
}

export function useChoroplethInteraction(): ChoroplethInteractionContextValue {
  const context = useContext(ChoroplethInteractionContext)
  if (!context) {
    throw new Error('useChoroplethInteraction must be used within a ChoroplethProvider')
  }
  return context
}

export function useChoropleth(): ChoroplethContextValue {
  return { ...useChoroplethStable(), ...useChoroplethInteraction() }
}

import { CHART_SCALE_VARS, chartScaleCssVars } from '../chart-scale'

// CSS variables for choropleth theming
export const choroplethCssVars = {
  scale01: chartScaleCssVars.scale01,
  scale02: chartScaleCssVars.scale02,
  scale03: chartScaleCssVars.scale03,
  scale04: chartScaleCssVars.scale04,
  scale05: chartScaleCssVars.scale05,
  patternColor: chartScaleCssVars.patternColor,
  stroke: 'var(--chart-grid)',
  background: 'var(--background)'
}

// Default colors array for cycling through features
export const defaultChoroplethColors = [...CHART_SCALE_VARS]
