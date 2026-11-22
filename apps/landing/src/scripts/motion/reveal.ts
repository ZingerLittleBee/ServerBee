/*
 * Scroll reveal. What is still below the screen when the page starts is held back, and each group appears
 * together, staggered, when any of its members nears the screen. What the reader can already see is left alone.
 */

type Kind = 'item' | 'shot' | 'text'

/** A group arrives once one of its members is this far up from the bottom of the screen. */
const LINE = 0.88
/** An observer's margin that ends the screen at that line. */
export const BELOW_LINE = `0px 0px -${Math.round((1 - LINE) * 100)}% 0px`

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
    // The kit arrives a group at a time: a title, then its tiles.
    for (const kit of all('.more-group')) {
      group(kit.querySelectorAll('h3, .more-tile'), 'item')
    }
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

  // Every member is watched, not only the first: a reader who jumps down the page and scrolls back up meets a
  // group's last member first, and it must not stay hidden until the first one comes into view.
  const groupOf = new Map<Element, HTMLElement[]>()
  const observer = new IntersectionObserver(
    (entries) => {
      for (const entry of entries) {
        if (entry.isIntersecting) {
          for (const member of groupOf.get(entry.target) ?? []) {
            observer.unobserve(member)
            groupOf.delete(member)
            member.dataset.in = ''
          }
        }
      }
    },
    { rootMargin: BELOW_LINE }
  )
  for (const members of groups) {
    for (const member of members) {
      groupOf.set(member, members)
      observer.observe(member)
    }
  }
}
