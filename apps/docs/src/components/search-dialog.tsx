import type { SortedResult } from 'fumadocs-core/search'
import { useDocsSearch } from 'fumadocs-core/search/client'
import {
  SearchDialog,
  SearchDialogClose,
  SearchDialogContent,
  SearchDialogHeader,
  SearchDialogIcon,
  SearchDialogInput,
  SearchDialogList,
  SearchDialogListItem,
  SearchDialogOverlay,
  type SearchItemType,
  type SharedProps,
  useSearchList
} from 'fumadocs-ui/components/dialog/search'
import { useI18n } from 'fumadocs-ui/contexts/i18n'
import { type KeyboardEvent, useEffect, useEffectEvent, useId, useRef, useState } from 'react'

import { i18n, isDocsLanguage } from '@/lib/i18n'
import { searchDialogText } from '@/lib/ui-translations'

/**
 * The results of each search URL. fumadocs' fetch client answers a query it has searched before with the array it got
 * then, which React takes for the results already shown, so going back to the query whose results were shown left the
 * status cleared. A copy arrives as new results.
 */
class SearchCache extends Map<string, SortedResult[]> {
  override get(url: string): SortedResult[] | undefined {
    const results = super.get(url)
    return results && [...results]
  }
}

const searchCache = new SearchCache()

interface ResultOptionProps {
  item: SearchItemType
  onActive: (optionId: string) => void
  onClick: () => void
  optionId: string
}

function ResultOption({ item, onActive, onClick, optionId }: ResultOptionProps) {
  const { active } = useSearchList()
  const isActive = active === item.id

  useEffect(() => {
    if (isActive) {
      onActive(optionId)
    }
  }, [isActive, onActive, optionId])

  // Options take no focus: focus stays in the input, which names the highlighted one as its active descendant.
  return <SearchDialogListItem id={optionId} item={item} onClick={onClick} role="option" tabIndex={-1} />
}

// fumadocs' search buttons. The docs sidebar has one in each of its halves, the docked sidebar and the floating panel
// shown once it is collapsed, and the half off screen is inert. The header has one too, shown only while the sidebar is
// not docked.
const searchButtons = '[data-search], [data-search-full]'

/** What focus goes back to once the dialog closes. */
interface Opener {
  /**
   * The controls of the open popups holding the element, innermost first. A popover, a menu or the table of contents
   * closes once focus moves into the dialog or a click lands outside it, and takes the element with it, and a menu can
   * hold another popup's control, as the 404 page's mobile menu holds the language menu's.
   */
  controls: HTMLElement[]
  /** The element focused before the dialog opened. */
  element: HTMLElement
  /** Whether the element was on screen. */
  onScreen: boolean
}

function isOnScreen(element: Element): boolean {
  const { bottom, left, right, top } = element.getBoundingClientRect()
  return bottom > 0 && right > 0 && top < window.innerHeight && left < window.innerWidth
}

/**
 * An element, with the control of each open popup holding it: the element naming the popup in aria-controls and
 * carrying aria-expanded=true. The sidebar toggles name the sidebar too, and carry no aria-expanded. Focus can be on a
 * popup itself, as on the page actions menu, whose items are links, which Radix does not focus when it opens.
 */
function openerOf(element: Element | null): Opener | null {
  if (!(element instanceof HTMLElement)) {
    return null
  }
  const controls: HTMLElement[] = []
  let node: HTMLElement | null = element
  while (node) {
    const control: HTMLElement | null = node.id
      ? document.querySelector<HTMLElement>(`[aria-controls="${CSS.escape(node.id)}"][aria-expanded="true"]`)
      : null
    if (control && !node.contains(control)) {
      controls.push(control)
      node = control
    }
    node = node.parentElement
  }
  return { controls, element, onScreen: isOnScreen(element) }
}

/** The controls that stand in for one that cannot take focus: the same control in the other half of the sidebar. */
function twinsOf(element: HTMLElement): HTMLElement[] {
  if (element.matches(searchButtons)) {
    return [...document.querySelectorAll<HTMLElement>(searchButtons)]
  }
  return element.closest('#nd-sidebar, [data-sidebar-panel]')
    ? [...document.querySelectorAll<HTMLElement>('[aria-controls="nd-sidebar"]')]
    : []
}

/**
 * Focuses the element focused before the dialog opened or, if its popup closed meanwhile, the popup's control, and
 * returns whether one took focus. One that cannot take focus, as when the half of the sidebar holding it went off
 * screen or a resize hid it, stands for its search button or sidebar toggle wherever one is shown.
 */
function restoreFocus(opener: Opener | null): boolean {
  if (!opener) {
    return false
  }
  for (const target of [opener.element, ...opener.controls]) {
    for (const candidate of [target, ...twinsOf(target)]) {
      candidate.focus({ preventScroll: true })
      if (document.activeElement === candidate) {
        // The page can move while the dialog is open, as on Back. Focus that was on screen stays on screen, and the
        // page stays where the reader left it otherwise.
        if (opener.onScreen && !isOnScreen(candidate)) {
          candidate.scrollIntoView({ block: 'center' })
        }
        return true
      }
    }
  }
  return false
}

/** fumadocs opens the highlighted result on an Enter anywhere in the window, so Enter on the close button did too. */
function keepEnterOnClose(event: KeyboardEvent<HTMLButtonElement>) {
  if (event.key === 'Enter') {
    event.stopPropagation()
  }
}

/**
 * fumadocs' default search dialog with listbox semantics. Its results are buttons carrying aria-selected, which ARIA
 * does not allow on buttons, and focus stays in the input while the arrow keys move the highlight, so a screen reader
 * never hears the highlighted result. Here the results are options of a listbox the input controls, and the
 * highlighted one is the input's active descendant.
 */
export default function DocsSearchDialog(props: SharedProps) {
  const { locale, text } = useI18n()
  const { search, setSearch, query } = useDocsSearch({ cache: searchCache, locale, type: 'fetch' })
  const listId = useId()
  const [activeId, setActiveId] = useState<string>()
  const items = query.data === 'empty' ? null : query.data
  const hasResults = Boolean(items && items.length > 0)
  const lang = locale && isDocsLanguage(locale) ? locale : i18n.defaultLanguage
  // What a screen reader announces once results arrive: how many, or that there are none. fumadocs also stops loading
  // when a search that a newer one replaced ends, with the older results still shown, so the status follows the
  // results, and is cleared each time a search starts. The language is read when results arrive: a change of language
  // starts a search, and announcing the results shown in the new language would announce them before its results.
  const [status, setStatus] = useState('')
  const describe = useEffectEvent((results: typeof items) => {
    if (!results) {
      return ''
    }
    return results.length > 0 ? searchDialogText[lang].resultCount(results.length) : text.searchNoResult
  })
  useEffect(() => {
    setStatus(describe(items))
  }, [items])
  useEffect(() => {
    if (query.isLoading) {
      setStatus('')
    }
  }, [query.isLoading])
  // fumadocs opens the dialog without a Radix trigger, the element Radix hands focus back to, so focus fell to the page
  // when the dialog closed. It goes back to the element focused before, unless a result was chosen, which navigates.
  const opener = useRef<Opener | null>(null)

  return (
    <SearchDialog
      isLoading={query.isLoading}
      onSearchChange={setSearch}
      onSelect={() => {
        opener.current = null
      }}
      search={search}
      {...props}
    >
      <SearchDialogOverlay />
      <SearchDialogContent
        onCloseAutoFocus={(event) => {
          if (restoreFocus(opener.current)) {
            event.preventDefault()
          }
        }}
        onOpenAutoFocus={() => {
          opener.current = openerOf(document.activeElement)
        }}
      >
        <SearchDialogHeader>
          <SearchDialogIcon />
          <SearchDialogInput
            aria-activedescendant={hasResults ? activeId : undefined}
            aria-autocomplete="list"
            aria-controls={listId}
            aria-expanded={hasResults}
            role="combobox"
          />
          <SearchDialogClose aria-label={searchDialogText[lang].closeSearch} onKeyDown={keepEnterOnClose} />
          {/* In the header, since the dialog draws a border under each of its parts but the last, the result list. */}
          <output aria-live="polite" className="sr-only">
            {status}
          </output>
        </SearchDialogHeader>
        <SearchDialogList
          Item={({ item, onClick }) => (
            <ResultOption
              item={item}
              onActive={setActiveId}
              onClick={onClick}
              optionId={`${listId}-${encodeURIComponent(item.id)}`}
            />
          )}
          id={listId}
          items={items}
          role={hasResults ? 'listbox' : undefined}
        />
      </SearchDialogContent>
    </SearchDialog>
  )
}
