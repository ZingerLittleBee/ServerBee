import { useTranslation } from 'react-i18next'
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip'
import { cn, countryCodeToFlag, countryCodeToName } from '@/lib/utils'

// Country flag emoji with a hover tooltip showing the localized country name, so users
// don't have to recognize a flag or its 2-letter code on sight. Renders nothing when the
// code is missing or invalid.
//
// The emoji itself stays decorative and unfocusable: it is redundant next to the server
// name and does not deserve a tab stop. The country name is exposed to assistive tech as
// sr-only text instead, which also folds it into the accessible name of the surrounding
// link when the flag sits inside one.
export function CountryFlag({ className, code }: { className?: string; code: string | null | undefined }) {
  const { i18n } = useTranslation()
  const flag = countryCodeToFlag(code)
  if (!flag) {
    return null
  }
  const name = countryCodeToName(code, i18n.language) || (code ?? '').toUpperCase()
  return (
    <>
      <Tooltip>
        <TooltipTrigger
          render={
            <span aria-hidden="true" className={cn('shrink-0', className)}>
              {flag}
            </span>
          }
        />
        <TooltipContent>{name}</TooltipContent>
      </Tooltip>
      <span className="sr-only">{name}</span>
    </>
  )
}
