const kSetUser = "SET_USER"

interface UserState {
  user?: string
}

interface UserAction {
  type: typeof kSetUser
  payload: string
}

const userReducer = (state: UserState, action: UserAction): UserState => {
  switch (action.type) {
    case kSetUser:
      return { ...state, user: action.payload }
    default:
      throw new Error(`Unknown action: ${action.type}`)
  }
}

export { userReducer }

export type { UserState, UserAction }
