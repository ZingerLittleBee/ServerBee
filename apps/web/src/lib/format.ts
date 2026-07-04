import i18next from 'i18next'

/**
 * Map the active app language (from the i18n language switcher) to a BCP-47
 * locale for Intl / toLocale* date formatting. Without this, dates fall back to
 * the browser locale (or a hardcoded 'en-US'), so a user viewing the app in
 * Chinese with an en-US browser sees mismatched US-style dates. Reading the
 * i18next singleton keeps these plain helpers usable outside React while still
 * tracking the current language (components re-render on switch via useTranslation).
 */
export function activeLocale(): string {
  const lang = i18next.resolvedLanguage ?? i18next.language ?? 'en'
  return lang.startsWith('zh') ? 'zh-CN' : 'en-US'
}

export function formatDate(date: Date | string | number | undefined, opts: Intl.DateTimeFormatOptions = {}) {
  if (!date) {
    return ''
  }

  try {
    return new Intl.DateTimeFormat(activeLocale(), {
      month: opts.month ?? 'long',
      day: opts.day ?? 'numeric',
      year: opts.year ?? 'numeric',
      ...opts
    }).format(new Date(date))
  } catch (_err) {
    return ''
  }
}

/** Date only, in the active locale. Drop-in for a bare `.toLocaleDateString()`. */
export function formatDateShort(
  date: Date | string | number | null | undefined,
  opts: Intl.DateTimeFormatOptions = {}
) {
  if (date == null) {
    return ''
  }

  try {
    return new Date(date).toLocaleDateString(activeLocale(), opts)
  } catch (_err) {
    return ''
  }
}

/** Date + time, in the active locale. Drop-in for a bare `.toLocaleString()`. */
export function formatDateTime(date: Date | string | number | null | undefined, opts: Intl.DateTimeFormatOptions = {}) {
  if (date == null) {
    return ''
  }

  try {
    return new Date(date).toLocaleString(activeLocale(), opts)
  } catch (_err) {
    return ''
  }
}
