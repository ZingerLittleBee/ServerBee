export class ZError extends Error {
  constructor(
    public path: string[],
    message: string
  ) {
    super(`${path.length ? path.join('.') + ': ' : ''}${message}`)
  }
}
