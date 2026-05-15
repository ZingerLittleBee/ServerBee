import { createFileRoute, useParams } from '@tanstack/react-router'
import { HomeLayout } from 'fumadocs-ui/layouts/home'

import { LandingPage } from '@/components/landing'
import type { LandingLang } from '@/components/landing/translations'
import { baseOptions } from '@/lib/layout.shared'

export const Route = createFileRoute('/$lang/')({
  component: Home
})

function Home() {
  const { lang } = useParams({ from: '/$lang/' })
  const landingLang: LandingLang = lang === 'cn' ? 'cn' : 'en'

  return (
    <HomeLayout {...baseOptions(lang)}>
      <LandingPage lang={landingLang} />
    </HomeLayout>
  )
}
