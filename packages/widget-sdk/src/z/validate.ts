export class ZError extends Error {
  path: string[]

  constructor(path: string[], message: string) {
    super(`${path.length ? `${path.join('.')}: ` : ''}${message}`)
    this.path = path
  }
}
