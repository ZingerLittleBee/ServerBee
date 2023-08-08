const KSetOpen = "SET_OPEN"

interface UserState {
  user?: string
}

interface UserAction {
  type: typeof KSetOpen
  payload: string
}

const userReducer = (state: UserState, action: UserAction): UserState => {
  switch (action.type) {
    case KSetOpen:
      return { ...state, user: action.payload }
    default:
      throw new Error(`Unknown action: ${action.type}`)
  }
}

export { userReducer }

export type { UserState, UserAction }
