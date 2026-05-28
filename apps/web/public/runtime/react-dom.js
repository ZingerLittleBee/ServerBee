const rd = globalThis.__SERVERBEE_REACT_DOM__
if (!rd) {
  throw new Error('react-dom shim: host did not mount __SERVERBEE_REACT_DOM__')
}
export default rd
export const createPortal = rd.createPortal
export const flushSync = rd.flushSync
