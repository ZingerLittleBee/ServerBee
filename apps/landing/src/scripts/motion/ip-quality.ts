/* IP quality: every server runs its checks, and the unlock matrix fills in cell by cell. */

import { el, grow, onArrival, overlay, place, rand, rectOf, regionsOf, sleep } from './stage'

/** The wave that resolves the matrix: how much later each next row and each next column starts. */
const ROW_STEP = 62
const COLUMN_STEP = 44

export function mountIpQuality(): void {
  const figure = document.querySelector<HTMLElement>('#ipq .shot')
  if (!figure) {
    return
  }
  const marks = regionsOf(figure).cells ?? []
  const first = marks[0]
  if (!first) {
    return
  }
  const columns = marks.filter((mark) => mark[1] === first[1]).length
  const rows = Math.floor(marks.length / columns)
  // Just left of the first result: the table's own ground.
  const scene = overlay(figure, 'ipq-layer', [[first[0] - 1.2, first[1] + first[3] / 2]])
  if (!scene) {
    return
  }
  // Each result is covered by a pending check until its turn comes.
  const cells = marks.map((mark) => {
    const cell = el('i', 'cover check')
    cell.dataset.tone = mark[4] ?? ''
    place(cell, grow(rectOf(mark), 0.3, scene.aspect))
    return cell
  })
  scene.layer.append(...cells)

  const resolve = (cell: HTMLElement, delay: number): void => {
    setTimeout(() => cell.setAttribute('data-done', ''), delay)
  }

  const visible = onArrival(figure, 0.3, async () => {
    await scene.ready
    await sleep(300)
    for (const [index, cell] of cells.entries()) {
      resolve(cell, Math.floor(index / columns) * ROW_STEP + (index % columns) * COLUMN_STEP + rand(0, 70))
    }
    await sleep(rows * ROW_STEP + columns * COLUMN_STEP + 900)
    // A server's exit IP changed: its row is checked again.
    while (figure.isConnected) {
      await sleep(rand(3800, 5600))
      if (!visible()) {
        continue
      }
      const row = Math.floor(rand(0, rows))
      for (let column = 0; column < columns; column++) {
        const cell = cells[row * columns + column]
        cell.removeAttribute('data-done')
        resolve(cell, 520 + column * 85)
      }
    }
  })
}
