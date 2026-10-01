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
import { type KeyboardEvent, useEffect, useId, useState } from 'react'

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
  const { locale } = useI18n()
  const { search, setSearch, query } = useDocsSearch({ type: 'fetch', locale })
  const listId = useId()
  const [activeId, setActiveId] = useState<string>()
  const items = query.data === 'empty' ? null : query.data
  const hasResults = Boolean(items && items.length > 0)

  return (
    <SearchDialog isLoading={query.isLoading} onSearchChange={setSearch} search={search} {...props}>
      <SearchDialogOverlay />
      <SearchDialogContent>
        <SearchDialogHeader>
          <SearchDialogIcon />
          <SearchDialogInput
            aria-activedescendant={hasResults ? activeId : undefined}
            aria-autocomplete="list"
            aria-controls={listId}
            aria-expanded={hasResults}
            role="combobox"
          />
          <SearchDialogClose onKeyDown={keepEnterOnClose} />
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
