declare module 'culori' {
  export interface Color {
    alpha?: number
    mode: string
  }

  export interface Oklch extends Color {
    c: number
    h?: number
    l: number
    mode: 'oklch'
  }

  export function converter(mode: 'oklch'): (color: Color | string) => Oklch | undefined
  export function formatHex(color: Color): string | undefined
  export function formatHex8(color: Color): string | undefined
  export function parse(color: string): Color | undefined
}
