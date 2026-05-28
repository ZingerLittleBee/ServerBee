const j = globalThis.__SERVERBEE_JSX_RUNTIME__
if (!j) {
  throw new Error('jsx-runtime shim: host did not mount __SERVERBEE_JSX_RUNTIME__')
}
export const jsx = j.jsx
export const jsxs = j.jsxs
export const Fragment = j.Fragment
