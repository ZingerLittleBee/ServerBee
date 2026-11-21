/*
 * Scroll reveal. What is still below the screen when the page starts is held back, and each group appears
 * together, staggered, when its first member nears the screen. What the reader can already see is left alone.
 */

type Kind = 'item' | 'shot' | 'text'

/** A group arrives once its first member is this far up from the bottom of the screen. */
const LINE = 0.88

export function mountReveal(): void {
  const groups: HTMLElement[][] = []
  const group = (elements: Iterable<Element | null>, kind: Kind, together = false): void => {
    const members = [...elements].filter((element) => element instanceof HTMLElement)
    // Read before anything is written, so the page is laid out once.
    if (members.length === 0 || members[0].getBoundingClientRect().top < innerHeight * LINE) {
      return
    }
    groups.push(members)
    pending.push(() => {
      for (const [index, member] of members.entries()) {
        // A heading stays put; its pieces arrive instead.
        member.dataset.reveal = member.matches('h2') ? 'pieces' : kind
        member.style.setProperty('--i', together ? '0' : String(index))
      }
    })
  }
  const pending: (() => void)[] = []

  for (const section of document.querySelectorAll('main > section:not(.hero)')) {
    const all = (selector: string): NodeListOf<Element> => section.querySelectorAll(selector)
    group(all('.ios-icon, h2, .facts, .lede, .feature-text > .out, :scope > .wrap > .hero-links'), 'text')
    const grant = section.querySelector('.grant')
    if (grant) {
      // The command and the table under it read as one object, so they arrive as one.
      group([grant, section.querySelector('.grant + .shot')], 'shot', true)
    } else {
      for (const shot of all('.shot')) {
        group([shot], 'shot')
      }
    }
    group(all('.phone'), 'shot')
    for (const list of all('.points')) {
      group(list.children, 'item')
    }
    group(all('.more-group'), 'item')
    group(all('.steps > li'), 'item')
    group(all('.flow > *'), 'item')
    group(all('.stats > div'), 'item')
    group(all('.notes'), 'text')
    group(all('details'), 'item')
    group(all(':scope > .wrap > .install'), 'text')
  }
  for (const write of pending) {
    write()
  }

  const observer = new IntersectionObserver(
    (entries) => {
      for (const entry of entries) {
        if (entry.isIntersecting) {
          observer.unobserve(entry.target)
          for (const member of groups.find((members) => members[0] === entry.target) ?? []) {
            member.dataset.in = ''
          }
        }
      }
    },
    { rootMargin: `0px 0px -${Math.round((1 - LINE) * 100)}% 0px` }
  )
  for (const members of groups) {
    observer.observe(members[0])
  }
}
