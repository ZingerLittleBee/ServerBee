"use client"

import * as React from "react"
import { useState } from "react"
import { addClientRequest } from "@/requests/client/client"
import { zodResolver } from "@hookform/resolvers/zod"
import { PlusCircle } from "lucide-react"
import { useForm } from "react-hook-form"
import * as z from "zod"

import { Button } from "@/components/ui/button"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@/components/ui/dialog"
import {
  Form,
  FormControl,
  FormField,
  FormItem,
  FormLabel,
  FormMessage,
} from "@/components/ui/form"
import { Input } from "@/components/ui/input"
import { useToast } from "@/components/ui/use-toast"
import { Icons } from "@/components/icons"
import { ClientDialog } from "@/app/client/components/client-dialog"

export function AddClient() {
  const { toast } = useToast()
  const [isOpen, setIsOpen] = useState<boolean>(false)
  const [isLoading, setIsLoading] = useState<boolean>(false)
  const [isClientDialogOpen, setIsClientDialogOpen] = useState<boolean>(false)

  const formSchema = z.object({
    name: z.string().nonempty({ message: "Not Empty" }).trim(),
  })

  const form = useForm<z.infer<typeof formSchema>>({
    resolver: zodResolver(formSchema),
    shouldUnregister: false,
    defaultValues: {
      name: "",
    },
  })

  async function onSubmit(data: z.infer<typeof formSchema>) {
    setIsLoading(true)
    const {
      success,
      data: clientVo,
      message,
    } = await addClientRequest(data.name)
    if (success) {
      toast({
        title: "Client added successfully.",
      })
    } else {
      toast({
        variant: "destructive",
        title: "Uh oh! Something went wrong.",
        description: message,
      })
    }
    // TODO
    console.log("clientVo", clientVo)
    setIsLoading(false)
    setIsOpen(false)
    setIsClientDialogOpen(true)
  }

  return (
    <>
      <Dialog open={isOpen} onOpenChange={setIsOpen}>
        <DialogTrigger asChild>
          <Button className="h-[32px]">
            <PlusCircle className="mr-2 h-4 w-4" /> Add Client
          </Button>
        </DialogTrigger>
        <DialogContent className="sm:max-w-[425px]">
          <DialogHeader>
            <DialogTitle>Add Client</DialogTitle>
            <DialogDescription>
              {"Add Client here. Click save when you're done."}
            </DialogDescription>
          </DialogHeader>
          <div className="grid gap-4 py-4">
            <Form {...form}>
              <form onSubmit={form.handleSubmit(onSubmit)}>
                <FormField
                  control={form.control}
                  name="name"
                  render={({ field }) => (
                    <div>
                      <FormItem>
                        <div className="grid grid-cols-4 items-center gap-4">
                          <FormLabel htmlFor="name" className="text-right">
                            Name
                          </FormLabel>
                          <FormControl>
                            <Input className="col-span-3" {...field} />
                          </FormControl>
                        </div>
                      </FormItem>
                      <FormMessage className="pt-2 text-right" />
                    </div>
                  )}
                />
              </form>
            </Form>
          </div>
          <DialogFooter>
            <Button
              type="submit"
              disabled={isLoading}
              onClick={form.handleSubmit(onSubmit)}
            >
              {isLoading && (
                <Icons.spinner className="mr-2 h-4 w-4 animate-spin" />
              )}
              Save changes
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
      <ClientDialog
        isOpen={isClientDialogOpen}
        setIsOpen={setIsClientDialogOpen}
      />
    </>
  )
}
