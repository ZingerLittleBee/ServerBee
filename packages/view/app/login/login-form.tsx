"use client"

import * as React from "react"
import { useState } from "react"
import { useRouter } from "next/navigation"
import { login } from "@/requests/auth/auth"
import { zodResolver } from "@hookform/resolvers/zod"
import { useForm } from "react-hook-form"
import useSWR from "swr"
import * as z from "zod"

import { cn } from "@/lib/utils"
import { Button } from "@/components/ui/button"
import {
  Form,
  FormControl,
  FormField,
  FormItem,
  FormLabel,
  FormMessage,
} from "@/components/ui/form"
import { Input } from "@/components/ui/input"
import { toast } from "@/components/ui/use-toast"
import { Icons } from "@/components/icons"

interface UserAuthFormProps extends React.HTMLAttributes<HTMLDivElement> {}

const formSchema = z
  .object({
    email: z.string().email().trim(),
    username: z
      .string()
      .regex(/^[a-zA-Z_]+$/, { message: "Username must start with letter" })
      .trim(),
    password: z
      .string()
      .min(8, { message: "Password must be at least 8 characters" }),
  })
  .or(
    z.object({
      username: z
        .string()
        .regex(/^[a-zA-Z].*$/, { message: "Username must start with letter" })
        .trim(),
      password: z
        .string()
        .min(8, { message: "Password must be at least 8 characters" }),
    })
  )
  .or(
    z.object({
      email: z.string().email().trim(),
      password: z
        .string()
        .min(8, { message: "Password must be at least 8 characters" }),
    })
  )

export function LoginForm({ className, ...props }: UserAuthFormProps) {
  const router = useRouter()
  const [isLoading, setIsLoading] = useState<boolean>(false)
  const [isEmail, setIsEmail] = useState<boolean>(true)

  const fetcher = (url: string) => fetch(url).then((r) => r.json())

  const { data, error, isLoading: load } = useSWR("/login/api/", fetcher)

  const form = useForm<z.infer<typeof formSchema>>({
    resolver: zodResolver(formSchema),
    shouldUnregister: false,
    defaultValues: {
      email: "",
      username: "",
      password: "",
    },
  })

  async function onSubmit(data: z.infer<typeof formSchema>) {
    console.log("submit", data.email, data.username, data.password)
    setIsLoading(true)
    const res = await login(data.password, data.email, data.username)
    if (res?.success) {
      router.push("/dashboard")
    } else {
      toast({
        title: "Error",
        description: res?.message,
      })
    }
    setIsLoading(false)
  }

  function onSwitchChange() {
    setIsEmail(!isEmail)
    form.setValue("email", "")
    form.setValue("username", "")
  }

  return (
    <div className={cn("grid gap-6", className)} {...props}>
      <div className="flex justify-center">
        {load ? <div>Loading...</div> : <div>{data}</div>}
      </div>
      <Form {...form}>
        <form onSubmit={form.handleSubmit(onSubmit)} className="grid gap-2">
          {isEmail ? (
            <FormField
              control={form.control}
              name="email"
              render={({ field }) => (
                <FormItem>
                  <FormLabel>Email</FormLabel>
                  <FormControl>
                    <Input
                      type="email"
                      autoComplete="email"
                      placeholder="name@example.com"
                      {...field}
                    />
                  </FormControl>
                  <FormMessage />
                </FormItem>
              )}
            />
          ) : (
            <FormField
              control={form.control}
              name="username"
              render={({ field }) => (
                <FormItem>
                  <FormLabel>Username</FormLabel>
                  <FormControl>
                    <Input
                      type="text"
                      autoComplete="username"
                      placeholder=""
                      {...field}
                    />
                  </FormControl>
                  <FormMessage />
                </FormItem>
              )}
            />
          )}
          <FormField
            control={form.control}
            name="password"
            render={({ field }) => (
              <FormItem>
                <FormLabel>Password</FormLabel>
                <FormControl>
                  <Input
                    type="password"
                    autoComplete="current-password"
                    placeholder=""
                    {...field}
                  />
                </FormControl>
                <FormMessage />
              </FormItem>
            )}
          />
          <Button type="submit" disabled={isLoading}>
            {isLoading && (
              <Icons.spinner className="mr-2 h-4 w-4 animate-spin" />
            )}
            {isEmail ? "Sign In with Email" : "Sign In with Username"}
          </Button>
        </form>
      </Form>
      <div className="relative">
        <div className="absolute inset-0 flex items-center">
          <span className="w-full border-t" />
        </div>
        <div className="relative flex justify-center text-xs uppercase">
          <span className="bg-background px-2 text-muted-foreground">
            Or continue with
          </span>
        </div>
      </div>
      <Button
        variant="outline"
        type="button"
        disabled={isLoading}
        onClick={onSwitchChange}
      >
        {isLoading ? (
          <Icons.spinner className="mr-2 h-4 w-4 animate-spin" />
        ) : isEmail ? (
          <Icons.user className="mr-2 h-4 w-4" />
        ) : (
          <Icons.mail className="mr-2 h-4 w-4" />
        )}
        {isEmail ? "Username" : "Email"}
      </Button>
    </div>
  )
}
