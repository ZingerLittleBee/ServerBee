"use client"

import React from "react"
import Link from "next/link"

import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog"
import { useClientStore } from "@/app/client/store"
import { kSetDialogIsOpen } from "@/app/client/store/dialog"

export interface ClientDialogProps {
  isOpen: boolean
  title: string
  description: React.ReactNode
}
// ClientCommunicationToken
export const cctTemplate = (
  token: string,
  title?: string
): ClientDialogProps => ({
  isOpen: true,
  title: title ?? "Client Communication Token",
  description: (
    <>
      This action cannot be undone. This will permanently delete your account
      and remove your data from our servers.
      <span className="mb-4 mt-6 block max-h-[650px] max-w-[460px] overflow-x-auto rounded-lg border bg-zinc-950 py-4 dark:bg-zinc-900">
        <code className="relative rounded px-[0.6rem] py-[0.2rem] font-mono text-sm">
          <span className="text-white">{token}</span>
        </code>
      </span>
      {"Don't know how to use?"}{" "}
      <Link
        href="https://docs.serverbee.app"
        target="_blank"
        className="underline"
      >
        Read the docs.
      </Link>
    </>
  ),
})

export function ClientDialog() {
  const { dialogState, dialogDispatch } = useClientStore()

  const { isOpen, props } = dialogState

  return (
    <AlertDialog
      open={isOpen}
      onOpenChange={(open: boolean) =>
        dialogDispatch({
          type: kSetDialogIsOpen,
          payload: open,
        })
      }
    >
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>{props.title}</AlertDialogTitle>
          <AlertDialogDescription>{props.description}</AlertDialogDescription>
        </AlertDialogHeader>
        <AlertDialogFooter>
          <AlertDialogCancel>Cancel</AlertDialogCancel>
          <AlertDialogAction>Continue</AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  )
}
