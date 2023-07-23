"use client"
import React, { createContext, ReactNode, useContext, useReducer } from "react";
import { UserAction, userReducer, UserState } from "@/store/user";

interface StateContextProps {
  userState: UserState;
  userDispatch: React.Dispatch<UserAction>;
}

const StateContext = createContext<StateContextProps | undefined>(undefined)

interface StateProviderProps {
    children: ReactNode;
}

const StateProvider = ({ children }: StateProviderProps): React.JSX.Element => {
  const [userState, userDispatch] = useReducer(userReducer, {})

  return (
    <StateContext.Provider
      value={{ userState, userDispatch }}
    >
      {children}
    </StateContext.Provider>
  )
}

// 创建一个自定义 hook 以方便使用
const useStore = () => useContext<StateContextProps | undefined>(StateContext)

export { StateProvider, useStore }
