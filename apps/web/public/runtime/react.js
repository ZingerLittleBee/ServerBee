const r = globalThis.__SERVERBEE_REACT__
if (!r) {
  throw new Error('react shim: host did not mount __SERVERBEE_REACT__')
}
export default r
export const useState = r.useState
export const useEffect = r.useEffect
export const useMemo = r.useMemo
export const useCallback = r.useCallback
export const useRef = r.useRef
export const useContext = r.useContext
export const useReducer = r.useReducer
export const useLayoutEffect = r.useLayoutEffect
export const createContext = r.createContext
export const Fragment = r.Fragment
export const memo = r.memo
export const forwardRef = r.forwardRef
export const Component = r.Component
export const Children = r.Children
export const cloneElement = r.cloneElement
export const createElement = r.createElement
export const isValidElement = r.isValidElement
