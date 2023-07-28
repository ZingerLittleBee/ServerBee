import { Metadata } from "next"

import { ClientStateProvider } from "@/app/client/store"

export const metadata: Metadata = {
  title: "Client",
  description: "Client page.",
}

interface ClientLayoutProps {
  children: React.ReactNode
}

export default function ClientLayout({ children }: ClientLayoutProps) {
  return (
    <section className="container grid items-center gap-6">
      <ClientStateProvider>{children}</ClientStateProvider>
    </section>
  )
}
