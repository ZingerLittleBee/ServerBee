import { Metadata } from "next"

import { Separator } from "@/components/ui/separator"

export const metadata: Metadata = {
  title: "Client",
  description: "Client page.",
}

interface ClientLayoutProps {
  children: React.ReactNode
}

export default function ClientLayout({ children }: ClientLayoutProps) {
  return (
    <section className="container grid items-center gap-6 pb-8">
      {children}
    </section>
  )
}
