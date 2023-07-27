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
import Link from "next/link";

interface ClientDialogProps {
  isOpen: boolean
  setIsOpen: (open: boolean) => void
}

export function ClientDialog({ isOpen, setIsOpen }: ClientDialogProps) {
  return (
    <AlertDialog open={isOpen} onOpenChange={setIsOpen}>
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>Client communication token</AlertDialogTitle>
          <AlertDialogDescription>
            This action cannot be undone. This will permanently delete your
            account and remove your data from our servers.
            <p className="mb-4 mt-6 max-h-[650px] max-w-[460px] overflow-x-auto rounded-lg border bg-zinc-950 py-4 dark:bg-zinc-900">
              <code className="relative rounded px-[0.6rem] py-[0.2rem] font-mono text-sm">
                <span className="text-white">jrwskdhfd2342342342342jskljadnqle3453453453494385084390wiejijwkqlk</span>
              </code>
            </p>
            {"Don't know how to use?"}{" "} <Link href="https://docs.serverbee.app"
                                                  target="_blank"
                                                  className="underline">Read the docs.</Link>
          </AlertDialogDescription>
        </AlertDialogHeader>
        <AlertDialogFooter>
          <AlertDialogCancel>Cancel</AlertDialogCancel>
          <AlertDialogAction>Continue</AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  )
}
