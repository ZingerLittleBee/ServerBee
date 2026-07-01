import { createFileRoute } from '@tanstack/react-router'
import { ChangePasswordSection } from './change-password-section'
import { OAuthAccountsSection } from './oauth-accounts-section'
import { TwoFactorSection } from './two-factor-section'

export const Route = createFileRoute('/_authed/settings/security')({
  component: SecurityPage
})

function SecurityPage() {
  return (
    <div>
      <div className="max-w-2xl space-y-8">
        <TwoFactorSection />
        <ChangePasswordSection />
        <OAuthAccountsSection />
      </div>
    </div>
  )
}
