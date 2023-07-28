"use client"

import React, { ReactNode, createContext, useContext, useReducer } from "react"

import { ClientDialogProps } from "@/app/client/components/client-dialog"
import {
  DialogAction,
  DialogState,
  dialogReducer,
} from "@/app/client/store/dialog"

interface ClientStateContextProps {
  dialogState: DialogState
  dialogDispatch: React.Dispatch<DialogAction>
}

const ClientStateContext = createContext<ClientStateContextProps>(
  {} as ClientStateContextProps
)

interface ClientStateProviderProps {
  children: ReactNode
}

const ClientStateProvider = ({
  children,
}: ClientStateProviderProps): React.JSX.Element => {
  const [dialogState, dialogDispatch] = useReducer(dialogReducer, {
    isOpen: false,
    props: {} as ClientDialogProps,
  })

  return (
    <ClientStateContext.Provider value={{ dialogState, dialogDispatch }}>
      {children}
    </ClientStateContext.Provider>
  )
}

const useClientStore = () =>
  useContext<ClientStateContextProps>(ClientStateContext)

export { ClientStateProvider, useClientStore }
