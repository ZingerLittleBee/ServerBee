/* The sources of a capture that follows the theme: one <picture> holding its light and dark variants. */

import { getImage } from 'astro:assets'
import type { ImageMetadata } from 'astro'

/**
 * AVIF first: at the same sharpness a capture weighs about half its WebP, and it keeps full chroma resolution, which
 * small colored text needs. WebP is the fallback for browsers without AVIF.
 */
const AVIF = { format: 'avif', quality: 60 }
const WEBP = { format: 'webp', quality: 90 }

export interface PictureSource {
  /** Applies under the dark theme only. src/scripts/landing.ts rewrites its `media` when a theme is chosen. */
  dark: boolean
  srcset: string
  type: string
}

export interface ThemedPicture {
  /** The light WebP: what <img> carries. */
  fallback: { src: string; srcset: string }
  /** In the order the browser must test them. */
  sources: PictureSource[]
}

export async function themedPicture(
  light: ImageMetadata,
  dark: ImageMetadata,
  widths: number[]
): Promise<ThemedPicture> {
  const [darkAvif, darkWebp, lightAvif, lightWebp] = await Promise.all([
    getImage({ src: dark, widths, ...AVIF }),
    getImage({ src: dark, widths, ...WEBP }),
    getImage({ src: light, widths, ...AVIF }),
    getImage({ src: light, widths, ...WEBP })
  ])
  return {
    fallback: { src: lightWebp.src, srcset: lightWebp.srcSet.attribute },
    sources: [
      { dark: true, srcset: darkAvif.srcSet.attribute, type: 'image/avif' },
      { dark: true, srcset: darkWebp.srcSet.attribute, type: 'image/webp' },
      { dark: false, srcset: lightAvif.srcSet.attribute, type: 'image/avif' }
    ]
  }
}
