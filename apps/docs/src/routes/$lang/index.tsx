import { createFileRoute, useParams } from '@tanstack/react-router'

import { LandingPage } from '@/components/landing'
import type { LandingLang } from '@/components/landing/translations'

export const Route = createFileRoute('/$lang/')({
  component: Home
})

function Home() {
  const { lang } = useParams({ from: '/$lang/' })
  const landingLang: LandingLang = lang === 'zh' ? 'zh' : 'en'

  return <LandingPage lang={landingLang} />
}
