import { ClientDialogProps } from "@/app/client/components/client-dialog"

export const kSetDialogIsOpen = "SET_DIALOG_IS_OPEN"
export const kOpenAlertDialog = "OPEN_ALERT_DIALOG"

export interface DialogState {
  isOpen: boolean
  props: ClientDialogProps
}

interface SetDialogIsOpenAction {
  type: typeof kSetDialogIsOpen
  payload: boolean
}

interface OpenAlertDialogAction {
  type: typeof kOpenAlertDialog
  payload: ClientDialogProps
}

export type DialogAction = SetDialogIsOpenAction | OpenAlertDialogAction

export const dialogReducer = (
  state: DialogState,
  action: DialogAction
): DialogState => {
  switch (action.type) {
    case kSetDialogIsOpen:
      return { ...state, isOpen: action.payload }
    case kOpenAlertDialog:
      return {
        ...state,
        isOpen: true,
        props: action.payload,
      }
    default:
      throw new Error(`Unknown action in dialogReducer: ${action}`)
  }
}
