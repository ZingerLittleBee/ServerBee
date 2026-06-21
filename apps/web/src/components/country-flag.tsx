import { useTranslation } from 'react-i18next'
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip'
import { cn, countryCodeToFlag, countryCodeToName } from '@/lib/utils'

// Country flag emoji with a hover tooltip showing the localized country name, so users
// don't have to recognize a flag or its 2-letter code on sight. Renders nothing when the
// code is missing or invalid.
export function CountryFlag({ className, code }: { className?: string; code: string | null | undefined }) {
  const { i18n } = useTranslation()
  const flag = countryCodeToFlag(code)
  if (!flag) {
    return null
  }
  const name = countryCodeToName(code, i18n.language) || (code ?? '').toUpperCase()
  return (
    <Tooltip>
      <TooltipTrigger
        render={
          <span aria-label={name} className={cn('shrink-0', className)} role="img">
            {flag}
          </span>
        }
      />
      <TooltipContent>{name}</TooltipContent>
    </Tooltip>
  )
}
